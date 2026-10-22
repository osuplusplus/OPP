//! 外部 CollectionManager shim 的进程调用适配。
//!
//! shim 是一个独立可执行文件（跨平台读取 lazer Realm 等场景由它代劳），
//! 协议为一次性 stdin/stdout JSON 报文；本文件负责组包、超时与错误码转换。
use super::CollectionManagerStatus;
use crate::infrastructure::logging::{finish_span, global};
use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    process::{Command, Stdio},
    time::Duration,
};
use tokio::time::timeout;

/// 为 [`Command`] 扩展“写入 stdin 后收集 stdout/stderr”的一次性调用语义。
trait CommandInput {
    fn output_with_input(self, input: Vec<u8>) -> std::io::Result<std::process::Output>;
}
impl CommandInput for Command {
    fn output_with_input(mut self, input: Vec<u8>) -> std::io::Result<std::process::Output> {
        use std::io::Write;
        let mut child = self.spawn()?;
        child.stdin.as_mut().unwrap().write_all(&input)?;
        child.wait_with_output()
    }
}

/// 发给 shim 的请求报文：`op` 为操作名，`payload` 可省略。
#[derive(Debug, Serialize)]
struct Request<'a, T> {
    op: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<T>,
    protocol_version: &'a str,
}
/// shim 的统一应答报文；`ok=false` 时错误信息在 `error` 字段。
#[derive(Debug, Deserialize)]
struct Response<T> {
    ok: bool,
    protocol_version: Option<String>,
    data: Option<T>,
    error: Option<String>,
}

/// 调用 shim 执行一次操作并反序列化 `data`。
///
/// 整个进程调用运行在阻塞线程上并以 10 秒为上限；
/// 路径未配置、进程失败、协议版本不匹配、`ok=false` 等情况分别映射为独立错误码。
pub async fn invoke<T: for<'de> Deserialize<'de>, P: Serialize>(
    state: &AppState,
    op: &str,
    payload: Option<P>,
) -> CommandResult<T> {
    let span = global().map(|logger| logger.operation("collection_manager", op));
    let settings = match state.store.settings_snapshot() {
        Ok(settings) => settings,
        Err(error) => return finish_span(span, Err(error)),
    };
    let path = match settings.collection_manager_path {
        Some(path) => path,
        None => {
            return finish_span(
                span,
                Err(CommandError::new(
                    "COLLECTION_MANAGER_NOT_CONFIGURED",
                    "未配置 CollectionManager shim",
                )),
            );
        }
    };
    let req = serde_json::to_vec(&Request {
        op,
        payload,
        protocol_version: "1",
    })?;
    let result = timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            // 一次性进程调用：请求整体写入 stdin，随后等待退出并收集输出。
            let mut c = Command::new(path);
            c.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            c.output_with_input(req)
        }),
    )
    .await
    .map_err(|_| CommandError::new("COLLECTION_MANAGER_TIMEOUT", "CollectionManager 响应超时"))
    .and_then(|r| r.map_err(|e| CommandError::new("COLLECTION_MANAGER_IO", e.to_string())))
    .and_then(|r| r.map_err(|e| CommandError::new("COLLECTION_MANAGER_IO", e.to_string())))
    .and_then(|out| {
        if !out.status.success() {
            return Err(CommandError::new(
                "COLLECTION_MANAGER_FAILED",
                String::from_utf8_lossy(&out.stderr),
            ));
        }
        let response: Response<T> = serde_json::from_slice(&out.stdout)
            .map_err(|e| CommandError::new("COLLECTION_MANAGER_INVALID_RESPONSE", e.to_string()))?;
        // 应答缺版本号时按兼容处理，显式返回其他版本才拒绝。
        if response
            .protocol_version
            .as_deref()
            .is_some_and(|v| v != "1")
        {
            return Err(CommandError::new(
                "COLLECTION_MANAGER_INCOMPATIBLE",
                "shim 协议版本不兼容",
            ));
        }
        if !response.ok {
            return Err(CommandError::new(
                "COLLECTION_MANAGER_OPERATION_FAILED",
                response.error.unwrap_or_else(|| "操作失败".into()),
            ));
        }
        response.data.ok_or_else(|| {
            CommandError::new("COLLECTION_MANAGER_INVALID_RESPONSE", "响应缺少 data")
        })
    });
    finish_span(span, result)
}

/// 探测 shim 可用性：未配置直接返回；已配置则通过 `validate` 操作握手，
/// 成功时汇报 shim 自述的版本与支持的操作列表，失败时保留错误信息。
pub async fn status(state: &AppState) -> CollectionManagerStatus {
    let configured = state
        .store
        .settings_snapshot()
        .ok()
        .and_then(|s| s.collection_manager_path)
        .is_some();
    if !configured {
        return CollectionManagerStatus {
            configured: false,
            available: false,
            protocol_version: None,
            version: None,
            operations: vec![],
            message: "未配置 CollectionManager shim".into(),
        };
    }
    match invoke::<serde_json::Value, ()>(state, "validate", None).await {
        Ok(v) => CollectionManagerStatus {
            configured: true,
            available: true,
            protocol_version: Some("1".into()),
            version: v
                .get("version")
                .and_then(|x| x.as_str())
                .map(str::to_string),
            operations: v
                .get("operations")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            message: "可用".into(),
        },
        Err(e) => CollectionManagerStatus {
            configured: true,
            available: false,
            protocol_version: None,
            version: None,
            operations: vec![],
            message: e.message,
        },
    }
}
