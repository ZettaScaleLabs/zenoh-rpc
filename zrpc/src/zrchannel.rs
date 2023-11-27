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

extern crate base64;
extern crate serde;

use async_std::sync::Arc;
use flume::Receiver;
use log::trace;
use serde::{Deserialize, Serialize};
use zenoh::prelude::r#async::*;
use zenoh::query::*;
use zenoh::Session;

use crate::request::Request;
use crate::response::Response;
use crate::serialize;
use crate::zrpcresult::{ZRPCError, ZRPCResult};

#[derive(Clone)]
pub struct ZRClientChannel {
    z: Arc<Session>,
    path: String,
    server_uuid: Option<ZenohId>,
}

impl ZRClientChannel {
    pub fn new(z: Arc<Session>, path: String, server_uuid: Option<ZenohId>) -> ZRClientChannel {
        ZRClientChannel {
            z,
            path,
            server_uuid,
        }
    }

    /// This functions calls the get on the workspace for the eval
    /// it serialized the request on the as properties in the selector
    /// the request is first serialized as json and then encoded in base64 and
    /// passed as a property named req
    async fn send<T>(&self, request: &Request<T>, method: &str) -> ZRPCResult<Receiver<Reply>>
    where
        T: Serialize + Clone + std::fmt::Debug,
        for<'de2> T: Deserialize<'de2>,
    {
        let req = serialize::serialize_request(&request)?;
        let selector = format!(
            "{}{}/eval?method_name={}",
            self.path,
            self.server_uuid.unwrap(),
            method
        );
        trace!("Sending {:?} to  {:?}", request, selector);
        Ok(self
            .z
            .get(&selector)
            .with_value(req)
            .target(QueryTarget::All)
            .res()
            .await?)
    }

    /// This function calls the eval on the server and deserialized the result
    /// if the value is not deserializable or the eval returns none it returns an IOError
    pub async fn call_fun<T, U>(&self, request: Request<T>, method: &str) -> ZRPCResult<Response<U>>
    where
        T: Serialize + Clone + std::fmt::Debug,
        for<'de2> T: Deserialize<'de2>,
        U: Serialize + Clone + std::fmt::Debug,
        for<'de3> U: Deserialize<'de3>,
    {
        let data_receiver = self.send(&request, method).await?;
        //takes only one, eval goes to only one
        let reply = data_receiver.recv_async().await;
        log::trace!("Response from zenoh is {:?}", reply);
        if let Ok(reply) = reply {
            match reply.sample {
                Ok(sample) => match sample.value.encoding {
                    Encoding::APP_OCTET_STREAM => {
                        let raw_data = sample.value.payload.contiguous().to_vec();
                        log::trace!("Size of response is {}", raw_data.len());
                        Ok(serialize::deserialize_response(&raw_data)?)
                    }
                    _ => Err(ZRPCError::ZenohError(
                        "Response data is expected to be APP_OCTET_STREAM in Zenoh!!".to_string(),
                    )),
                },
                Err(e) => {
                    log::error!("Unable to get sample from {e:?}");
                    Err(ZRPCError::ZenohError(format!(
                        "Unable to get sample from {e:?}"
                    )))
                }
            }
        } else {
            log::error!("No data from server");
            Err(ZRPCError::ZenohError(format!(
                "No data from call_fun for Request {:?}",
                request
            )))
        }
    }

    /// This function verifies is the server is still available to reply at requests
    /// it first verifies that it is register in Zenoh, then it verifies if the peer is still connected,
    /// and then verifies the state, it returns an std::io::Result, the Err case describe the error.
    pub async fn verify_server(&self) -> ZRPCResult<bool> {
        if self.server_uuid.is_none() {
            return Ok(false);
        }

        log::trace!(
            "Check server selector {}",
            format!("{}{}/state", self.path, self.server_uuid.unwrap())
        );
        let selector = format!("{}{}/state", self.path, self.server_uuid.unwrap());
        let replies = self.z.get(&selector).target(QueryTarget::All).res().await?;

        let reply = replies.recv_async().await;
        log::trace!("Response from zenoh is {:?}", reply);

        if let Ok(reply) = reply {
            match reply.sample {
                Ok(sample) => match sample.value.encoding {
                    Encoding::APP_OCTET_STREAM => {
                        let ca = crate::serialize::deserialize_state::<crate::types::ComponentState>(
                            &sample.value.payload.contiguous(),
                        )?;
                        if ca.status == crate::types::ComponentStatus::SERVING {
                            return Ok(true);
                        }
                    }
                    _ => {
                        return Err(ZRPCError::ZenohError(
                            "Server information is not correctly encoded".to_string(),
                        ))
                    }
                },
                Err(e) => {
                    log::error!("Unable to get sample from {e:?}");
                    return Err(ZRPCError::ZenohError(format!(
                        "Unable to get sample from {e:?}"
                    )));
                }
            }
        }
        Ok(false)
    }
}
