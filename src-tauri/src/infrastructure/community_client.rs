//! Public transport capability for features sharing the VPS device session.
//! 供多个 feature 复用同一 VPS 设备会话的传输抽象：实现方（如 BeatmapHub
//! 服务）持有会话与错误映射，业务侧只面向路径与请求体编程。
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::CommandResult;

/// 社区服务共用的 HTTP 通道接口，由持有设备会话的服务实现，
/// 使 community 等业务模块不依赖具体客户端。
pub(crate) trait CommunityClient {
    /// 无认证 GET，用于公开端点。
    async fn public_get<T: DeserializeOwned>(&self, path: &str) -> CommandResult<T>;
    /// 携带设备会话认证的 JSON 请求；`body` 为 `None` 时不带请求体。
    async fn authenticated_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> CommandResult<T>;
    /// 携带认证的 DELETE，成功时无返回体。
    async fn authenticated_delete(&self, path: &str) -> CommandResult<()>;
}
