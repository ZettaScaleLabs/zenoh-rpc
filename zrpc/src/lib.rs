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
#![allow(clippy::upper_case_acronyms)]

mod request;
mod response;
mod result;
mod rpcchannel;
mod serialize;
mod server;
mod service;
mod status;
mod types;
mod zrpcresult;

pub mod prelude {
    pub use crate::request::Request;
    pub use crate::response::Response;
    pub use crate::rpcchannel::RPCClientChannel;
    pub use crate::serialize::{deserialize, serialize};
    pub use crate::server::Server;
    pub use crate::service::Service;
    pub use crate::status::{Code, Status};
    pub use crate::types::Message;
}
