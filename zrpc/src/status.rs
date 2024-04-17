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

    pub fn code(&self) -> &Code {
        &self.code
    }

    pub fn message(&self) -> &String {
        &self.message
    }

    pub fn ok<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::Ok, message)
    }

    pub fn created<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::Created, message)
    }

    pub fn bad_request<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::BadRequest, message)
    }

    pub fn unauthorized<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::Unauthorized, message)
    }

    pub fn forbidden<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::Forbidden, message)
    }

    pub fn not_found<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::NotFound, message)
    }

    pub fn timeout<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::Timeout, message)
    }

    pub fn internal_error<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::InternalError, message)
    }

    pub fn not_implemented<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::NotImplemented, message)
    }

    pub fn unavailable<IntoString>(message: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        Self::new(Code::Unavailable, message)
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
