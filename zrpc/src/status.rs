use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Status {
    code: Code,
    message: String,
    metadata: HashMap<String, String>,
}

impl Status {
    pub fn new<IntoString>(code: Code, message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self {
            code,
            message: message.into(),
            metadata: HashMap::new(),
        }
    }
}

/// Zenoh-RPC status codes
/// Based on HTTP ones: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status#client_error_responses
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    Ok = 200,
    Created = 201,
    Accepted = 202,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    Timeout = 408,
    InternalError = 500,
    NotImplemented = 501,
    Unavailable = 503,
}
