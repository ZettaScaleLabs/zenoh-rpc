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
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;
use zenoh::key_expr::format::KeFormat;
use zenoh::prelude::ZenohId;

use zrpc::prelude::*;

use std::collections::HashSet;
use std::ops::Deref;
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
        Ok(HelloResponse::from(name).into())
    }

    async fn add(&self, _request: Request<AddRequest>) -> Result<Response<AddResponse>, Status> {
        let mut guard = self.counter.lock().await;
        *guard += 1;
        let value = *guard;
        Ok(AddResponse::from(value).into())
    }

    async fn sub(&self, _request: Request<SubRequest>) -> Result<Response<SubResponse>, Status> {
        Err(Status::not_implemented("Not yet!"))
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
                // Box::pin(async move { Err(Status::unavailable "Unavailable")) })
                Err(Status::unavailable("Unavailable"))
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

impl From<HelloRequest> for Request<HelloRequest> {
    fn from(value: HelloRequest) -> Self {
        Request::new(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddRequest {}

impl From<AddRequest> for Request<AddRequest> {
    fn from(value: AddRequest) -> Self {
        Request::new(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubRequest {}

impl From<SubRequest> for Request<SubRequest> {
    fn from(value: SubRequest) -> Self {
        Request::new(value)
    }
}

/// The response sent over the wire from the server to the client.
/// generated code

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloResponse(String);

impl From<HelloResponse> for Response<HelloResponse> {
    fn from(value: HelloResponse) -> Self {
        Response::new(value)
    }
}

impl From<String> for HelloResponse {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl Deref for HelloResponse {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddResponse(u64);

impl From<u64> for AddResponse {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<AddResponse> for Response<AddResponse> {
    fn from(value: AddResponse) -> Self {
        Response::new(value)
    }
}

impl Deref for AddResponse {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubResponse(u64);

impl From<u64> for SubResponse {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<SubResponse> for Response<SubResponse> {
    fn from(value: SubResponse) -> Self {
        Response::new(value)
    }
}

impl Deref for SubResponse {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct HelloClientBuilder<'a> {
    pub z: Arc<Session>,
    pub labels: HashSet<String>,
    ke_format: KeFormat<'a>,
    pub tout: Duration,
}

impl<'a> HelloClientBuilder<'a> {
    pub fn add_label<IntoString>(mut self, label: IntoString) -> Self
    where
        IntoString: Into<String>,
    {
        self.labels.insert(label.into());
        self
    }

    pub fn labels<IterIntoString, IntoString>(mut self, labels: IterIntoString) -> Self
    where
        IntoString: Into<String>,
        IterIntoString: Iterator<Item = IntoString>,
    {
        self.labels.extend(labels.map(|e| e.into()));
        self
    }

    pub fn timeout(mut self, tout: Duration) -> Self {
        self.tout = tout;
        self
    }

    pub fn build(self) -> HelloClient<'a> {
        HelloClient {
            ch: RPCClientChannel::new(self.z.clone(), "Hello"),
            ke_format: self.ke_format,
            z: self.z,
            tout: self.tout,
            labels: self.labels,
        }
    }
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub struct HelloClient<'a> {
    pub(crate) ch: RPCClientChannel,
    pub(crate) ke_format: KeFormat<'a>,
    pub(crate) z: Arc<Session>,
    pub(crate) tout: Duration,
    pub(crate) labels: HashSet<String>,
}

// generated client code

impl<'a> HelloClient<'a> {
    pub fn builder(z: Arc<zenoh::Session>) -> HelloClientBuilder<'a> {
        HelloClientBuilder {
            z,
            labels: HashSet::new(),
            ke_format: KeFormat::new("@rpc/${zid:*}/service/Hello").unwrap(),
            tout: std::time::Duration::from_secs(60u16 as u64),
        }
    }

    pub async fn hello<IntoRequest>(
        &self,
        request: IntoRequest,
    ) -> Result<Response<HelloResponse>, Status>
    where
        IntoRequest: Into<Request<HelloRequest>>,
    {
        self.ch
            .call_fun(
                self.find_server().await?,
                request.into(),
                "hello",
                self.tout,
            )
            .await
    }

    pub async fn add<IntoRequest>(
        &self,
        request: IntoRequest,
    ) -> Result<Response<AddResponse>, Status>
    where
        IntoRequest: Into<Request<AddRequest>>,
    {
        self.ch
            .call_fun(self.find_server().await?, request.into(), "add", self.tout)
            .await
    }

    pub async fn sub<IntoRequest>(
        &self,
        request: IntoRequest,
    ) -> Result<Response<SubResponse>, Status>
    where
        IntoRequest: Into<Request<SubRequest>>,
    {
        self.ch
            .call_fun(self.find_server().await?, request.into(), "sub", self.tout)
            .await
    }

    // this could be improved by caching the server id
    // maybe by using this https://github.com/moka-rs/moka
    async fn find_server(&self) -> Result<ZenohId, Status> {
        let res = self
            .z
            .liveliness()
            .get("@rpc/*/service/Hello")
            .res()
            .await
            .map_err(|e| {
                Status::unavailable(format!("Unable to perform liveliness query: {e:?}"))
            })?;

        let ids = res
            .into_iter()
            .map(|e| {
                self.extract_id_from_ke(
                    &e.sample
                        .map_err(|_| Status::unavailable("Cannot get value from sample"))?
                        .key_expr,
                )
            })
            .collect::<Result<Vec<ZenohId>, Status>>()?;

        // get server metadata
        let metadatas = self.ch.get_servers_metadata(&ids, self.tout).await?;

        // filter the metadata based on labels
        let mut ids: Vec<ZenohId> = metadatas
            .into_iter()
            .filter(|m| m.labels.is_superset(&self.labels))
            .map(|m| m.id)
            .collect();

        ids.pop().ok_or(Status::unavailable("No servers found"))
    }

    fn extract_id_from_ke(&self, ke: &KeyExpr) -> Result<ZenohId, Status> {
        let id_str = self
            .ke_format
            .parse(ke)
            .map_err(|e| Status::internal_error(format!("Unable to parse key expression: {e:?}")))?
            .get("zid")
            .map_err(|e| {
                Status::internal_error(format!(
                    "Unable to get server id from key expression: {e:?}"
                ))
            })?
            .ok_or(Status::unavailable(
                "Unable to get server id from key expression: Option is None",
            ))?;

        ZenohId::from_str(id_str)
            .map_err(|e| Status::internal_error(format!("Unable to convert str to ZenohId: {e:?}")))
    }
}

#[tokio::main]
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

        tokio::task::spawn(async move {
            let service = MyServer {
                ser_name: "test service".to_string(),
                counter: Arc::new(Mutex::new(0u64)),
            };
            let builder =
                zrpc::prelude::Server::builder(z).add_service(Arc::new(HelloServer::new(service)));

            let _ = builder.build().serve().await;
        });

        let client = HelloClient::builder(zsession).build();

        press_to_continue().await;

        let hello = client
            .hello(HelloRequest {
                name: "client".to_string(),
            })
            .await;
        println!("Res is: {:?}", hello);

        press_to_continue().await;
        let res = client.add(AddRequest {}).await;
        println!("Res is: {:?}", res);

        press_to_continue().await;
        let res = client.add(AddRequest {}).await;
        println!("Res is: {:?}", res);

        press_to_continue().await;
        let res = client.add(AddRequest {}).await;
        println!("Res is: {:?}", res);

        press_to_continue().await;
        let res = client.sub(SubRequest {}).await;
        println!("Res is: {:?}", res);
    }
}

async fn press_to_continue() {
    println!("Press ENTER to continue...");
    let buffer = &mut [0u8];
    tokio::io::stdin().read_exact(buffer).await.unwrap();
}
