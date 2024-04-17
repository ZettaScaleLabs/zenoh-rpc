/*********************************************************************************
* Copyright (c) 2022 ZettaScale Technology
*
* This program and the accompanying materials are made available under the
* terms of the Eclipse Public License 2.0 which is available at
* http://www.eclipse.org/legal/epl-2.0, or the Apache Software License 2.0
* which is available at https://www.apache.org/licenses/LICENSE-2.0.
*
* SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
* Contributors:
*   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
*********************************************************************************/

use crate::{
    request::Request,
    response::Response,
    serialize::serialize,
    status::{Code, Status},
};
use futures::Future;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
};
use zenoh::config::ZenohId;

pub(crate) type ServerTaskFuture =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, Status>> + Send + 'static>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WireMessage {
    pub(crate) payload: Option<Vec<u8>>,
    pub(crate) status: Status,
}

#[derive(Debug)]
pub struct Message {
    pub method: String,
    pub body: Vec<u8>,
    pub metadata: HashMap<String, String>,
    pub status: Status,
}

impl Message {
    pub fn from_parts(
        method: String,
        body: Vec<u8>,
        metadata: HashMap<String, String>,
        status: Status,
    ) -> Self {
        Self {
            method,
            body,
            metadata,
            status,
        }
    }
}

impl From<Status> for Message {
    fn from(value: Status) -> Self {
        Self {
            method: "".into(),
            body: vec![],
            metadata: HashMap::new(),
            status: value,
        }
    }
}

impl<T> From<Response<T>> for Message
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    fn from(value: Response<T>) -> Self {
        Self {
            method: "".into(),
            body: serialize(value.get_ref()).unwrap(),
            metadata: value.get_metadata().clone(),
            status: Status::new(Code::Ok, ""),
        }
    }
}

impl<T> From<Request<T>> for Message
where
    T: Serialize + Clone + std::fmt::Debug,
    for<'de2> T: Deserialize<'de2>,
{
    fn from(value: Request<T>) -> Self {
        Self {
            method: "".into(),
            body: serialize(value.get_ref()).unwrap(),
            metadata: value.get_metadata().clone(),
            status: Status::new(Code::Ok, ""),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerMetadata {
    pub labels: HashSet<String>,
    pub id: ZenohId,
}
