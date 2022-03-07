/*********************************************************************************
* Copyright (c) 2018,2020 ADLINK Technology Inc.
*
* This program and the accompanying materials are made available under the
* terms of the Eclipse Public License 2.0 which is available at
* http://www.eclipse.org/legal/epl-2.0, or the Apache Software License 2.0
* which is available at https://www.apache.org/licenses/LICENSE-2.0.
*
* SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
* Contributors:
*   ADLINK fog05 team, <fog05@adlink-labs.tech>
*********************************************************************************/

extern crate base64;
extern crate serde;

use async_std::sync::Arc;
use futures::prelude::*;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;
use uuid::Uuid;
use zenoh::net::protocol::io::SplitBuffer;
use zenoh::prelude::Encoding;
use zenoh::query::{Reply, ReplyReceiver};
use zenoh::Session;

use log::trace;

use crate::serialize;
use crate::zrpcresult::{ZRPCError, ZRPCResult};

#[derive(Clone)]
pub struct ZClientChannel<Req, Resp> {
    z: Arc<Session>,
    path: String,
    server_uuid: Option<Uuid>,
    phantom_resp: PhantomData<Resp>,
    phantom_req: PhantomData<Req>,
}

impl<Req, Resp> ZClientChannel<Req, Resp>
where
    Resp: DeserializeOwned,
    Req: std::fmt::Debug + Serialize,
{
    pub fn new(
        z: Arc<Session>,
        path: String,
        server_uuid: Option<Uuid>,
    ) -> ZClientChannel<Req, Resp> {
        ZClientChannel {
            z,
            path,
            server_uuid,
            phantom_resp: PhantomData,
            phantom_req: PhantomData,
        }
    }

    /// This functions calls the get on the workspace for the eval
    /// it serialized the request on the as properties in the selector
    /// the request is first serialized as json and then encoded in base64 and
    /// passed as a property named req
    async fn send(&self, request: &Req) -> ZRPCResult<ReplyReceiver> {
        let req = serialize::serialize_request(&request)?;
        let selector = format!(
            "{}/{}/eval?(req={})",
            self.path,
            self.server_uuid.unwrap(),
            base64::encode(req)
        );
        //Should create the appropriate Error type and the conversions form ZError
        trace!("Sending {:?} to  {:?}", request, selector);
        Ok(self.z.get(&selector).await?)
    }

    /// This function calls the eval on the server and deserialized the result
    /// if the value is not deserializable or the eval returns none it returns an IOError
    pub async fn call_fun(&self, request: Req) -> ZRPCResult<Resp> {
        let mut data_receiver = self.send(&request).await?;
        //takes only one, eval goes to only one
        let reply = data_receiver.next().await;
        log::trace!("Response from zenoh is {:?}", reply);
        if let Some(reply) = reply {
            let sample = reply.data;
            match sample.value.encoding {
                Encoding::APP_OCTET_STREAM => {
                    let raw_data = sample.value.payload.contiguous().to_vec();
                    log::trace!("Size of response is {}", raw_data.len());
                    Ok(serialize::deserialize_response(&raw_data)?)
                }
                _ => Err(ZRPCError::ZenohError(
                    "Response data is expected to be RAW in Zenoh!!".to_string(),
                )),
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

        let selector = format!("{}/{}/state", self.path, self.server_uuid.unwrap());

        let ds = self.z.get(&selector).await?;
        let mut idata: Vec<Reply> = ds.collect().await;

        if idata.is_empty() {
            return Ok(false);
        }

        let reply = idata.remove(0);
        let sample = reply.data;
        match sample.value.encoding {
            Encoding::APP_OCTET_STREAM => {
                let raw_data = sample.value.payload.contiguous().to_vec();
                log::trace!("Size of state is {}", raw_data.len());
                let cs = serialize::deserialize_state::<super::ComponentState>(&raw_data)?;
                let selector = format!("/@/router/{}", String::from(&cs.routerid));
                let ds = self.z.get(&selector).await?;

                let mut rdata: Vec<Reply> = ds.collect().await;

                if rdata.len() != 1 {
                    return Err(ZRPCError::NotFound);
                }

                let rreply = rdata.remove(0);
                let rsample = rreply.data;
                match rsample.value.encoding {
                    Encoding::APP_JSON => {
                        log::trace!(
                            "Size of Zenoh router state is {}",
                            rsample.value.payload.len()
                        );
                        let sv = String::from_utf8(rsample.value.payload.contiguous().to_vec())?;
                        let ri = serde_json::from_str::<super::types::ZRouterInfo>(&sv)?;
                        let mut it = ri.sessions.iter();
                        let f = it.find(|&x| x.peer == String::from(&cs.peerid).to_uppercase());

                        if f.is_none() {
                            return Ok(false);
                        }

                        match cs.status {
                            super::ComponentStatus::SERVING => Ok(true),
                            _ => Ok(false),
                        }
                    }
                    _ => Err(ZRPCError::ZenohError(
                        "Router information is not encoded in JSON".to_string(),
                    )),
                }
            }
            _ => Err(ZRPCError::ZenohError(
                "Component state is expected to be RAW in Zenoh!!".to_string(),
            )),
        }
    }
}
