//! Request/notification dispatcher helpers.
//!
//! Thin wrappers around `lsp_server::Request::extract` to reduce
//! boilerplate in `server.rs`.

use anyhow::Result;
use lsp_server::{Request, RequestId};
use serde::de::DeserializeOwned;

/// Extract typed params from a request, returning `(id, params)`.
pub fn extract<P: DeserializeOwned>(req: Request, method: &str) -> Result<(RequestId, P)> {
    req.extract(method).map_err(|e| anyhow::anyhow!("{e:?}"))
}
