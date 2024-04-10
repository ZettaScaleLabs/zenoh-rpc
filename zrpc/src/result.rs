use crate::{response::Response, status::Status};
use serde::{Deserialize, Serialize};
use std::convert::From;

#[derive(Serialize, Debug, Deserialize, Clone)]
#[serde(bound = "T: Serialize, for<'de2> T: Deserialize<'de2>")]
pub enum RPCResult<T>
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    Ok(Response<T>),
    Err(Status),
}

impl<T> From<Result<Response<T>, Status>> for RPCResult<T>
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    fn from(value: Result<Response<T>, Status>) -> Self {
        match value {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::Err(e),
        }
    }
}

impl<T> Into<Result<Response<T>, Status>> for RPCResult<T>
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    fn into(self) -> Result<Response<T>, Status> {
        match self {
            Self::Ok(r) => Ok(r),
            Self::Err(s) => Err(s),
        }
    }
}
