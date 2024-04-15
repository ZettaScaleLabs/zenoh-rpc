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

use async_std::io::ReadExt;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use zenoh::key_expr::format::KeFormat;

use zenoh::prelude::ZenohId;

use zrpc::prelude::*;

use std::str::{self, FromStr};
use std::time::Duration;
use zenoh::{Session, SessionDeclarations};

use serde::{Deserialize, Serialize};
use zenoh::prelude::r#async::*;

// this is the user defined trait
#[async_trait::async_trait]
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

#[async_trait::async_trait]
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

unsafe impl<T: Hello> Send for HelloServer<T> {}
unsafe impl<T: Hello> Sync for HelloServer<T> {}

#[async_trait::async_trait]
impl<T> zrpc::prelude::Service for HelloServer<T>
where
    T: Hello + Send + Sync + 'static,
{
    async fn call(&self, req: Message) -> Result<Message, Status> {
        match req.method.as_str() {
            "hello" => {
                let req = zrpc::prelude::deserialize::<Request<HelloRequest>>(&req.body).unwrap();
                match self.inner.hello(req).await {
                    Ok(resp) => Ok(resp.into()),
                    Err(s) => Err(s),
                }
            }
            "add" => {
                let req = zrpc::prelude::deserialize::<Request<AddRequest>>(&req.body).unwrap();
                match self.inner.add(req).await {
                    Ok(resp) => Ok(resp.into()),
                    Err(s) => Err(s),
                }
            }
            "sub" => {
                let req = zrpc::prelude::deserialize::<Request<SubRequest>>(&req.body).unwrap();
                match self.inner.sub(req).await {
                    Ok(resp) => Ok(resp.into()),
                    Err(s) => Err(s),
                }
            }

            _ => {
                // Box::pin(async move { Err(Status::new(Code::Unvailable, "Unavailable")) })
                Err(Status::new(Code::Unvailable, "Unavailable"))
            }
        }
    }

    fn name(&self) -> String {
        "Hello".into()
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
pub struct HelloClient<'a> {
    ch: RPCClientChannel,
    ke_format: KeFormat<'a>,
    z: Arc<Session>,
    tout: Duration,
}

// generated client code

impl<'a> HelloClient<'a> {
    pub async fn new(z: async_std::sync::Arc<zenoh::Session>) -> HelloClient<'a> {
        let new_client = RPCClientChannel::new(z.clone(), "Hello".into());
        let tout = std::time::Duration::from_secs(60u16 as u64);
        let ke_format = KeFormat::new("@rpc/${zid:*}/service/Hello").unwrap();
        HelloClient {
            ch: new_client,
            ke_format,
            z,
            tout,
        }
    }

    pub async fn hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        self.ch
            .call_fun(self.find_server().await, request, "hello", self.tout)
            .await
            .into()
    }

    pub async fn add(&self, request: Request<AddRequest>) -> Result<Response<AddResponse>, Status> {
        self.ch
            .call_fun(self.find_server().await, request, "add", self.tout)
            .await
            .into()
    }

    pub async fn sub(&self, request: Request<SubRequest>) -> Result<Response<SubResponse>, Status> {
        self.ch
            .call_fun(self.find_server().await, request, "sub", self.tout)
            .await
            .into()
    }

    async fn find_server(&self) -> ZenohId {
        let res = self
            .z
            .liveliness()
            .get("@rpc/*/service/Hello")
            .res()
            .await
            .unwrap();

        let mut ids: Vec<ZenohId> = res
            .into_iter()
            .map(|e| self.extract_id_from_ke(e.sample.unwrap().key_expr()))
            .collect();
        ids.pop().unwrap()
    }

    fn extract_id_from_ke(&self, ke: &KeyExpr) -> ZenohId {
        self.ke_format
            .parse(ke)
            .unwrap()
            .get("zid")
            .map(ZenohId::from_str)
            .unwrap()
            .unwrap()
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

        task::spawn(async move {
            let service = MyServer {
                ser_name: "test service".to_string(),
                counter: Arc::new(Mutex::new(0u64)),
            };
            let mut server = zrpc::prelude::Server::new(z);
            server.add_service(Arc::new(HelloServer::new(service)));
            server.serve().await;
        });

        let client = HelloClient::new(zsession).await;

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
    }
}

async fn press_to_continue() {
    println!("Press ENTER to continue...");
    let buffer = &mut [0u8];
    async_std::io::stdin().read_exact(buffer).await.unwrap();
}
