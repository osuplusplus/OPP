//! Public transport capability for features sharing the VPS device session.
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::CommandResult;

pub(crate) trait CommunityClient {
    async fn public_get<T: DeserializeOwned>(&self, path: &str) -> CommandResult<T>;
    async fn authenticated_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> CommandResult<T>;
    async fn authenticated_delete(&self, path: &str) -> CommandResult<()>;
}
