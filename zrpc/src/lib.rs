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
//#![feature(associated_type_bounds)]
#![allow(clippy::upper_case_acronyms)]

pub use futures::stream::{AbortHandle, AbortRegistration, Abortable, Aborted};
use request::Request;
use response::Response;
use serde::{Deserialize, Serialize};
use serialize::{deserialize_request, serialize_request, serialize_response};
use status::{Code, Status};

use std::{collections::HashMap, future::Future, hash::Hash, sync::Arc, time::Duration};
pub mod zchannel;
pub mod zrchannel;
pub use zchannel::ZClientChannel;

pub mod types;
pub use types::*;

pub mod request;
pub mod response;
pub mod result;
pub mod rpcchannel;
pub mod serialize;
pub mod status;
pub mod zrpcresult;

use zenoh::{key_expr::format::KeFormat, prelude::r#async::*};
use zrpcresult::ZRPCResult;

pub type BoxFuture<T, E> = std::pin::Pin<Box<dyn self::Future<Output = Result<T, E>> + 'static>>;

pub struct Server {
    session: Arc<Session>,
    services: HashMap<String, Arc<dyn Service>>,
}

impl Server {
    pub fn new(session: Arc<Session>) -> Self {
        Self {
            session,
            services: HashMap::new(),
        }
    }

    pub fn add_service(&mut self, svc: Arc<dyn Service>) {
        self.services.insert(svc.name(), svc);
    }

    pub fn instance_uuid(&self) -> ZenohId {
        self.session.zid()
    }

    pub async fn serve(&self) -> () {
        let mut tokens = vec![];
        // register the queryables and declare a liveliness token
        let ke = format!("@rpc/{}/**", self.instance_uuid());

        let ke_format =
            KeFormat::new("@rpc/${zid:*}/service/${service_name:*}/${method_name:*}").unwrap();
        let queryable = self.session.declare_queryable(&ke).res().await.unwrap();

        for k in self.services.keys() {
            let ke = format!("@rpc/{}/service/{k}", self.instance_uuid());
            let lt = self
                .session
                .liveliness()
                .declare_token(ke)
                .res()
                .await
                .unwrap();
            tokens.push(lt)
        }

        loop {
            let query = queryable.recv_async().await.unwrap();

            let selector = query.selector();

            // here we should get the name of the service and from it the name of the service
            // and the name of the method, we use the ke formatter
            // println!("Received selector: {selector:?}");

            let parsed = ke_format.parse(&selector.key_expr).unwrap();

            let service_name = parsed.get("service_name").unwrap();
            let method_name = parsed.get("method_name").unwrap();
            // println!("Calling {service_name}/{method_name}");
            let svc = self.services.get(service_name).unwrap();

            let payload: Vec<u8> = query.payload().unwrap().into();
            // let attachments : HashMap<String, String> = query.attachment().unwrap().into();
            let msg = Message {
                method: method_name.into(),
                body: payload,
                metadata: HashMap::new(),
                status: Status::new(Code::Accepted, ""),
            };

            let resp = svc.call(msg).await;
            // println!("Response is {resp:?}");
            let resp = match resp {
                Ok(msg) => {
                    let wmsg = WireMessage {
                        payload: Some(msg.body),
                        status: Status::new(Code::Ok, ""),
                    };

                    serialize_response(&wmsg)
                }
                Err(e) => {
                    let wmsg = WireMessage {
                        payload: None,
                        status: e,
                    };
                    serialize_response(&wmsg)
                }
            }
            .unwrap();
            query.reply(selector.key_expr, resp).res().await.unwrap();
        }
    }
}

pub trait Service {
    // type Response;
    // type Error;

    // type Future: Future<Output = Result<Message, Status>>;
    fn call(&self, req: Message) -> BoxFuture<Message, Status>;

    fn name(&self) -> String;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WireMessage {
    payload: Option<Vec<u8>>,
    status: Status,
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
            body: serialize_response(value.get_ref()).unwrap(),
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
            body: serialize_request(value.get_ref()).unwrap(),
            metadata: value.get_metadata().clone(),
            status: Status::new(Code::Ok, ""),
        }
    }
}

/// Trait to be implemented by services
pub trait ZServe<Req>: Sized + Clone {
    /// Type of the response
    type Resp;

    fn instance_uuid(&self) -> ZenohId;

    /// Connects to Zenoh, do nothing in this case, state is HALTED
    #[allow(clippy::type_complexity)]
    fn connect(
        &self,
    ) -> ::core::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = ZRPCResult<(
                        futures::stream::AbortHandle,
                        async_std::task::JoinHandle<Result<ZRPCResult<()>, Aborted>>,
                    )>,
                > + '_,
        >,
    >;

    /// Authenticates to Zenoh, state changes to INITIALIZING
    #[allow(clippy::type_complexity)]
    fn initialize(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    // Registers, state changes to REGISTERED
    #[allow(clippy::type_complexity)]
    fn register(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    // // // Announce, state changes to ANNOUNCED
    // // //fn announce(&self);

    /// The actual run loop serving the queriable
    #[allow(clippy::type_complexity)]
    fn run(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    /// State changes to SERVING, calls serve on a task::spawn, returns a stop sender and the serve task handle
    #[allow(clippy::type_complexity)]
    fn start(
        &self,
    ) -> ::core::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = ZRPCResult<(
                        futures::stream::AbortHandle,
                        async_std::task::JoinHandle<Result<ZRPCResult<()>, Aborted>>,
                    )>,
                > + '_,
        >,
    >;

    /// Starts serving all requests
    #[allow(clippy::type_complexity)]
    fn serve(
        &self,
        barrier: async_std::sync::Arc<async_std::sync::Barrier>,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    /// State changes to REGISTERED, will stop serve/work
    #[allow(clippy::type_complexity)]
    fn stop(
        &self,
        stop: futures::stream::AbortHandle,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    // state changes to HALTED
    #[allow(clippy::type_complexity)]
    fn unregister(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;

    /// removes state from Zenoh
    #[allow(clippy::type_complexity)]
    fn disconnect(
        &self,
        stop: futures::stream::AbortHandle,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>>;
}
