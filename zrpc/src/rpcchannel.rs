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
use crate::result::RPCResult;
use crate::serialize;
use crate::serialize::deserialize_response;
use crate::status::Code;
use crate::status::Status;
use crate::zrpcresult::{ZRPCError, ZRPCResult};
use crate::WireMessage;

#[derive(Clone, Debug)]
pub struct RPCClientChannel {
    z: Arc<Session>,
    service_name: String,
}

impl RPCClientChannel {
    pub fn new(z: Arc<Session>, service_name: String) -> RPCClientChannel {
        RPCClientChannel { z, service_name }
    }

    /// This functions calls the get on the workspace for the eval
    /// it serialized the request on the as properties in the selector
    /// the request is first serialized as json and then encoded in base64 and
    /// passed as a property named req
    async fn send<T>(
        &self,
        server_id: ZenohId,
        request: &Request<T>,
        method: &str,
    ) -> ZRPCResult<Receiver<Reply>>
    where
        T: Serialize + Clone + std::fmt::Debug,
        for<'de2> T: Deserialize<'de2>,
    {
        let req = serialize::serialize_request(&request)?;
        let selector = format!(
            "@rpc/{}/service/{}/{}",
            server_id, self.service_name, method
        );
        trace!("Sending {:?} to  {:?}", request, selector);
        Ok(self
            .z
            .get(&selector)
            .value(req)
            .target(QueryTarget::All)
            .res()
            .await?)
    }

    /// This function calls the eval on the server and deserialized the result
    /// if the value is not deserializable or the eval returns none it returns an IOError
    pub async fn call_fun<T, U>(
        &self,
        server_id: ZenohId,
        request: Request<T>,
        method: &str,
    ) -> RPCResult<U>
    where
        T: Serialize + Clone + std::fmt::Debug,
        for<'de2> T: Deserialize<'de2>,
        U: Serialize + Clone + std::fmt::Debug,
        for<'de3> U: Deserialize<'de3>,
    {
        let data_receiver = self.send(server_id, &request, method).await.unwrap();
        //takes only one, eval goes to only one
        let reply = data_receiver.recv_async().await;
        log::trace!("Response from zenoh is {:?}", reply);
        if let Ok(reply) = reply {
            match reply.sample {
                Ok(sample) => {
                    let raw_data: Vec<u8> = sample.payload().into();
                    let wmsg: WireMessage = deserialize_response(&raw_data).unwrap();
                    // println!("Wire MSG is {:?}", wmsg);
                    match wmsg.payload {
                        Some(raw_data) => match serialize::deserialize_response::<U>(&raw_data) {
                            Ok(r) => {
                                // println!("Data is {:?}", r);
                                RPCResult::Ok(Response::new(r))
                            }
                            Err(_) => RPCResult::Err(wmsg.status),
                        },
                        None => RPCResult::Err(wmsg.status),
                    }
                }
                Err(e) => {
                    log::error!("Unable to get sample from {e:?}");
                    RPCResult::Err(Status::new(
                        Code::InternalError,
                        &format!("Unable to get sample from {e:?}"),
                    ))
                }
            }
        } else {
            log::error!("No data from server");
            RPCResult::Err(Status::new(
                Code::InternalError,
                &format!("No data from call_fun for Request {:?}", request),
            ))
        }
    }
}
