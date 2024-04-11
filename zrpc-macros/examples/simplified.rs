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

// #![feature(prelude_import)]
#![allow(clippy::manual_async_fn)]
#![allow(clippy::large_enum_variant)]
// #[prelude_import]
extern crate serde;
extern crate std;

use std::prelude::v1::*;

use async_std::io::ReadExt;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use syn::token::As;
use zenoh::prelude::ZenohId;
use zenoh::queryable::Query;
use zrpc::result::RPCResult;
use zrpc::rpcchannel::RPCClientChannel;
use zrpc::{BoxFuture, Server};

use std::str;
use std::time::Duration;
use zenoh::{query, Session};

use serde::{Deserialize, Serialize};
use zrpc::zrpcresult::{ZRPCError, ZRPCResult};
use zrpc::{Message, ZServe};

use async_trait::async_trait;
use zrpc::request::Request;
use zrpc::response::Response;
use zrpc::status::{Code, Status};

// this is the user defined trait
#[async_trait]
pub trait Hello {
    async fn hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status>;
    async fn add(&self, request: Request<AddRequest>) -> Result<Response<AddResponse>, Status>;
    async fn sub(&self, request: Request<SubRequest>) -> Result<Response<SubResponse>, Status>;
}

// user code
#[derive(Clone, Debug)]
struct MyServer {
    pub ser_name: String,
    pub counter: Arc<Mutex<u64>>,
}

// user code

#[async_trait]
impl Hello for MyServer {
    async fn hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        let name = format!(
            "Hello {}!, you are connected to {}",
            request.get_ref().name,
            self.ser_name
        );
        Ok(Response::new(HelloResponse { name }))
    }

    async fn add(&self, _request: Request<AddRequest>) -> Result<Response<AddResponse>, Status> {
        let mut guard = self.counter.lock().await;
        *guard += 1;
        let value = *guard;
        Ok(Response::new(AddResponse { value }))
    }

    async fn sub(&self, _request: Request<SubRequest>) -> Result<Response<SubResponse>, Status> {
        Err(Status::new(Code::NotImplemented, "Not yet!"))
    }
}

// generated code
#[derive(Debug)]
pub struct HelloServer<T: Hello> {
    inner: Arc<T>,
}

impl<T> HelloServer<T>
where
    T: Hello,
{
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<T> zrpc::Service for HelloServer<T>
where
    T: Hello + 'static,
{
    // type Response = Message;

    // type Error = Status;

    // type Future = BoxFuture<Self::Response, Self::Error>;

    fn call(&self, req: Message) -> BoxFuture<Message, Status> {
        // extract selector
        // let selector = req.selector();
        // // getting the query parameters
        // let parameters = selector.parameters_cowmap().unwrap();

        // // getting method name
        // let method = parameters.get("method_name").unwrap().to_string();

        match req.method.as_str() {
            "hello" => {
                // let raw_value: Vec<u8> = value.payload().into();
                let req = zrpc::serialize::deserialize_request::<Request<HelloRequest>>(&req.body)
                    .unwrap();
                let inner = self.inner.clone();
                let fut = async move {
                    match inner.hello(req).await {
                        Ok(resp) => Ok(resp.into()),
                        Err(s) => Err(s),
                    }
                };
                Box::pin(fut)
            }
            "add" => {
                // let raw_value: Vec<u8> = value.payload().into();
                let req =
                    zrpc::serialize::deserialize_request::<Request<AddRequest>>(&req.body).unwrap();
                let inner = self.inner.clone();
                let fut = async move {
                    match inner.add(req).await {
                        Ok(resp) => Ok(resp.into()),
                        Err(s) => Err(s),
                    }
                };
                Box::pin(fut)
            }
            "sub" => {
                // let raw_value: Vec<u8> = value.payload().into();
                let req =
                    zrpc::serialize::deserialize_request::<Request<SubRequest>>(&req.body).unwrap();
                let inner = self.inner.clone();
                let fut = async move {
                    match inner.sub(req).await {
                        Ok(resp) => Ok(resp.into()),
                        Err(s) => Err(s),
                    }
                };
                Box::pin(fut)
            }

            _ => Box::pin(async move { Err(Status::new(Code::Unvailable, "Unavailable")) }),
        }
    }

    fn name(&self) -> String {
        return "Hello".into();
    }
}

/// The requests sent over the wire from the client to the server
/// generated code

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloRequest {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddRequest {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubRequest {}

/// The response sent over the wire from the server to the client.
/// generated code

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloResponse {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddResponse {
    pub value: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubResponse {
    pub value: u64,
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub struct HelloClient {
    ch: RPCClientChannel,
    server_uuid: ZenohId,
}

// generated client code

impl HelloClient {
    pub fn new(z: async_std::sync::Arc<zenoh::Session>, instance_id: ZenohId) -> HelloClient {
        let new_client = RPCClientChannel::new(z, "Hello".into(), Some(instance_id));
        HelloClient {
            ch: new_client,
            server_uuid: instance_id,
        }
    }
    pub fn get_server_uuid(&self) -> ZenohId {
        self.server_uuid
    }

    pub async fn hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        let resp = self.ch.call_fun(request, "hello");
        let dur = std::time::Duration::from_secs(60u16 as u64);
        match async_std::future::timeout(dur, resp).await {
            Ok(r) => r.into(),
            Err(e) => Err(Status::new(Code::Timeout, "")),
        }
    }

    pub async fn add(&self, request: Request<AddRequest>) -> Result<Response<AddResponse>, Status> {
        let resp = self.ch.call_fun(request, "add");
        let dur = std::time::Duration::from_secs(60u16 as u64);
        match async_std::future::timeout(dur, resp).await {
            Ok(r) => r.into(),
            Err(e) => Err(Status::new(Code::Timeout, "")),
        }
    }

    pub async fn sub(&self, request: Request<SubRequest>) -> Result<Response<SubResponse>, Status> {
        let resp = self.ch.call_fun(request, "sub");
        let dur = std::time::Duration::from_secs(60u16 as u64);
        match async_std::future::timeout(dur, resp).await {
            Ok(r) => r.into(),
            Err(e) => Err(Status::new(Code::Timeout, "")),
        }
    }
}

#[async_std::main]
async fn main() {
    {
        env_logger::init();
        use zenoh::prelude::r#async::*;

        let mut config = zenoh::config::Config::default();
        config
            .set_mode(Some(zenoh::config::whatami::WhatAmI::Peer))
            .unwrap();
        let zsession = Arc::new(zenoh::open(config).res().await.unwrap());

        let z = zsession.clone();
        let ser_uuid = zsession.zid();
        println!("Server instance UUID {}", ser_uuid);
        let client = HelloClient::new(zsession.clone(), ser_uuid);

        let service = MyServer {
            ser_name: "test service".to_string(),
            counter: Arc::new(Mutex::new(0u64)),
        };

        let mut server = Server::new(z);
        server.add_service(Arc::new(HelloServer::new(service)));

        task::spawn(async move {
            press_to_continue().await;

            let hello = client
                .hello(Request::new(HelloRequest {
                    name: "client".to_string(),
                }))
                .await;
            println!("Res is: {:?}", hello);

            press_to_continue().await;
            let res = client.add(Request::new(AddRequest {})).await;
            println!("Res is: {:?}", res);

            press_to_continue().await;
            let res = client.add(Request::new(AddRequest {})).await;
            println!("Res is: {:?}", res);

            press_to_continue().await;
            let res = client.add(Request::new(AddRequest {})).await;
            println!("Res is: {:?}", res);

            press_to_continue().await;
            let res = client.sub(Request::new(SubRequest {})).await;
            println!("Res is: {:?}", res);
        });

        server.serve().await;
    }
}

async fn press_to_continue() {
    println!("Press ENTER to continue...");
    let buffer = &mut [0u8];
    async_std::io::stdin().read_exact(buffer).await.unwrap();
}
