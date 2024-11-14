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

use std::time::Duration;

use serde::{Deserialize, Serialize};
use zenoh::config::ZenohId;
use zenoh::handlers::FifoChannelHandler;
use zenoh::query::*;
use zenoh::Session;

use crate::request::Request;
use crate::response::Response;
use crate::serialize::{deserialize, serialize};
use crate::status::Code;
use crate::status::Status;
use crate::types::ServerMetadata;
use crate::types::WireMessage;
use crate::zrpcresult::ZRPCResult;

#[derive(Clone, Debug)]
pub struct RPCClientChannel {
    z: Session,
    service_name: String,
}

impl RPCClientChannel {
    pub fn new<IntoString>(z: Session, service_name: IntoString) -> RPCClientChannel
    where
        IntoString: Into<String>,
    {
        RPCClientChannel {
            z,
            service_name: service_name.into(),
        }
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
        tout: Duration,
    ) -> ZRPCResult<FifoChannelHandler<Reply>>
    where
        T: Serialize + Clone + std::fmt::Debug,
        for<'de2> T: Deserialize<'de2>,
    {
        let req = serialize(&request)?;
        let selector = format!(
            "@rpc/{}/service/{}/{}",
            server_id, self.service_name, method
        );
        tracing::debug!("Sending {:?} to  {:?}", request, selector);
        Ok(self
            .z
            .get(&selector)
            .payload(req)
            .target(QueryTarget::All)
            .consolidation(ConsolidationMode::None)
            .timeout(tout)
            .await?)
    }

    /// This function calls the eval on the server and deserialized the result
    /// if the value is not deserializable or the eval returns none it returns an IOError
    pub async fn call_fun<T, U>(
        &self,
        server_id: ZenohId,
        request: Request<T>,
        method: &str,
        tout: Duration,
    ) -> Result<Response<U>, Status>
    where
        T: Serialize + Clone + std::fmt::Debug,
        for<'de2> T: Deserialize<'de2>,
        U: Serialize + Clone + std::fmt::Debug,
        for<'de3> U: Deserialize<'de3>,
    {
        tracing::debug!("calling function: {request:?}");
        let data_receiver = self
            .send(server_id, &request, method, tout)
            .await
            .map_err(|e| Status::new(Code::InternalError, format!("communication error: {e:?}")))?;
        //takes only one, eval goes to only one
        let reply = data_receiver.recv_async().await;
        tracing::debug!("Response from zenoh is {:?}", reply);
        if let Ok(reply) = reply {
            match reply.result() {
                Ok(sample) => {
                    // This is infallible, using unwrap_or_default so that if cannot get
                    // the vec then the deseriazliation fail on an empty one.
                    let raw_data = sample.payload().to_bytes().to_vec();

                    let wmsg: WireMessage = deserialize(&raw_data).map_err(|e| {
                        Status::new(Code::InternalError, format!("deserialization error: {e:?}"))
                    })?;
                    // println!("Wire MSG is {:?}", wmsg);
                    match wmsg.payload {
                        Some(raw_data) => match deserialize::<U>(&raw_data) {
                            Ok(r) => {
                                // println!("Data is {:?}", r);
                                Ok(Response::new(r))
                            }
                            Err(_) => Err(wmsg.status),
                        },
                        None => Err(wmsg.status),
                    }
                }
                Err(e) => {
                    tracing::error!("Unable to get sample from {e:?}");
                    Err(Status::new(
                        Code::InternalError,
                        format!("Unable to get sample from {e:?}"),
                    ))
                }
            }
        } else {
            tracing::error!("No data from server");
            Err(Status::new(
                Code::InternalError,
                format!("No data from call_fun for Request {:?}", request),
            ))
        }
    }

    pub async fn get_servers_metadata(
        &self,
        ids: &[ZenohId],
        tout: Duration,
    ) -> Result<Vec<ServerMetadata>, Status> {
        let mut tot_metadata = Vec::with_capacity(ids.len());

        for id in ids {
            //do one query per id... not efficient
            let ke = format!("@rpc/{id}/metadata");
            let data = self
                .z
                .get(ke)
                .target(QueryTarget::All)
                .consolidation(ConsolidationMode::None)
                .timeout(tout)
                .await
                .map_err(|e| {
                    Status::new(Code::InternalError, format!("communication error: {e:?}"))
                })?;
            let metadata = data
                .into_iter()
                // getting only reply with Sample::Ok
                .filter_map(|r| r.into_result().ok())
                // getting only the ones we can deserialize
                .filter_map(|s| {
                    // This is infallible
                    let raw_data = s.payload().to_bytes().to_vec();
                    deserialize::<WireMessage>(&raw_data).ok()
                })
                // get only the ones that do not have errors
                .filter_map(|wmgs| wmgs.payload)
                // get the ones where we can actually deserialize the payload
                .filter_map(|pl| deserialize::<ServerMetadata>(&pl).ok())
                // filter by the IDs providing the needed service
                .filter(|m| ids.contains(&m.id))
                .collect::<Vec<ServerMetadata>>();

            tot_metadata.extend_from_slice(&metadata);
        }
        Ok(tot_metadata)
    }
}
