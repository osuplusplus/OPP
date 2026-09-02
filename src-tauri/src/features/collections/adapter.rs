use std::{process::{Command, Stdio}, time::Duration};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use crate::{error::{CommandError, CommandResult}, state::AppState};
use super::CollectionManagerStatus;

trait CommandInput { fn output_with_input(self, input: Vec<u8>) -> std::io::Result<std::process::Output>; }
impl CommandInput for Command { fn output_with_input(mut self, input: Vec<u8>) -> std::io::Result<std::process::Output> { use std::io::Write; let mut child=self.spawn()?; child.stdin.as_mut().unwrap().write_all(&input)?; child.wait_with_output() } }

#[derive(Debug, Serialize)] struct Request<'a, T> { op: &'a str, #[serde(skip_serializing_if="Option::is_none")] payload: Option<T>, protocol_version: &'a str }
#[derive(Debug, Deserialize)] struct Response<T> { ok: bool, protocol_version: Option<String>, version: Option<String>, #[serde(default)] operations: Vec<String>, data: Option<T>, error: Option<String> }

pub async fn invoke<T: for<'de> Deserialize<'de>, P: Serialize>(state: &AppState, op: &str, payload: Option<P>) -> CommandResult<T> {
    let settings = state.store.snapshot()?.settings;
    let path = settings.collection_manager_path.ok_or_else(|| CommandError::new("COLLECTION_MANAGER_NOT_CONFIGURED", "未配置 CollectionManager shim"))?;
    let req = serde_json::to_vec(&Request { op, payload, protocol_version: "1" })?;
    let out = timeout(Duration::from_secs(10), tokio::task::spawn_blocking(move || { let mut c=Command::new(path); c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()); c.output_with_input(req) })).await.map_err(|_| CommandError::new("COLLECTION_MANAGER_TIMEOUT", "CollectionManager 响应超时"))?.map_err(|e| CommandError::new("COLLECTION_MANAGER_IO", e.to_string()))?.map_err(|e| CommandError::new("COLLECTION_MANAGER_IO", e.to_string()))?;
    if !out.status.success() { return Err(CommandError::new("COLLECTION_MANAGER_FAILED", String::from_utf8_lossy(&out.stderr))); }
    let response: Response<T> = serde_json::from_slice(&out.stdout).map_err(|e| CommandError::new("COLLECTION_MANAGER_INVALID_RESPONSE", e.to_string()))?;
    if response.protocol_version.as_deref().is_some_and(|v| v != "1") { return Err(CommandError::new("COLLECTION_MANAGER_INCOMPATIBLE", "shim 协议版本不兼容")); }
    if !response.ok { return Err(CommandError::new("COLLECTION_MANAGER_OPERATION_FAILED", response.error.unwrap_or_else(|| "操作失败".into()))); }
    response.data.ok_or_else(|| CommandError::new("COLLECTION_MANAGER_INVALID_RESPONSE", "响应缺少 data"))
}

pub async fn status(state: &AppState) -> CollectionManagerStatus {
    let configured = state.store.snapshot().ok().and_then(|s| s.settings.collection_manager_path).is_some();
    if !configured { return CollectionManagerStatus { configured:false, available:false, protocol_version:None, version:None, operations:vec![], message:"未配置 CollectionManager shim".into() }; }
    match invoke::<serde_json::Value, ()>(state, "validate", None).await {
        Ok(v) => CollectionManagerStatus { configured:true, available:true, protocol_version:Some("1".into()), version:v.get("version").and_then(|x| x.as_str()).map(str::to_string), operations:v.get("operations").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(), message:"可用".into() },
        Err(e) => CollectionManagerStatus { configured:true, available:false, protocol_version:None, version:None, operations:vec![], message:e.message },
    }
}
