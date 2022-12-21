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

use async_std::sync::{Arc, Mutex};
use async_std::task;

use std::str;
use std::time::Duration;
use uuid::Uuid;
use zenoh::Session;

use serde::{Deserialize, Serialize};
use zrpc::zrpcresult::{ZRPCError, ZRPCResult};
use zrpc::ZServe;

pub trait Hello: Clone {
    fn hello(
        &self,
        name: String,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = String> + core::marker::Send + '_>>;
    fn add(
        &mut self,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = u64> + core::marker::Send + '_>>;
    /// Returns the server object
    fn get_hello_server(
        self,
        z: async_std::sync::Arc<zenoh::Session>,
        id: Option<uuid::Uuid>,
    ) -> ServeHello<Self> {
        let id = id.unwrap_or_else(Uuid::new_v4);
        ServeHello::new(z, self, id)
    }
}

#[derive(Clone, Debug)]
pub struct ServeHello<S> {
    z: async_std::sync::Arc<zenoh::Session>,
    server: S,
    instance_id: uuid::Uuid,
    state: async_std::sync::Arc<async_std::sync::RwLock<zrpc::ComponentState>>,
}

impl<S> ServeHello<S> {
    pub fn new(z: async_std::sync::Arc<zenoh::Session>, server: S, id: uuid::Uuid) -> Self {
        let ci = zrpc::ComponentState {
            uuid: id,
            name: "HelloService".to_string(),
            routerid: "".to_string(),
            peerid: "".to_string(),
            status: zrpc::ComponentStatus::HALTED,
        };
        Self {
            z,
            server,
            instance_id: id,
            state: async_std::sync::Arc::new(async_std::sync::RwLock::new(ci)),
        }
    }
}
impl<S> zrpc::ZServe<HelloRequest> for ServeHello<S>
where
    S: Hello + Send + 'static,
{
    type Resp = HelloResponse;
    fn instance_uuid(&self) -> uuid::Uuid {
        self.instance_id
    }

    #[allow(unused)]
    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn connect(
        &'_ self,
    ) -> ::core::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = ZRPCResult<(
                        zrpc::AbortHandle,
                        async_std::task::JoinHandle<Result<ZRPCResult<()>, zrpc::Aborted>>,
                    )>,
                > + '_,
        >,
    > {
        async fn __connect<S>(
            _self: &ServeHello<S>,
        ) -> ZRPCResult<(
            zrpc::AbortHandle,
            async_std::task::JoinHandle<Result<ZRPCResult<()>, zrpc::Aborted>>,
        )>
        where
            S: Hello + Send + 'static,
        {
            use futures::prelude::*;
            use std::convert::TryInto;
            use zenoh::prelude::r#async::*;

            let zinfo = _self.z.info();
            let pid = zinfo.zid().res().await.to_string().to_uppercase();

            let rid = match zinfo
                .routers_zid()
                .res()
                .await
                .collect::<Vec<ZenohId>>()
                .first()
            {
                Some(head) => head.to_string().to_uppercase(),
                None => "".to_string(),
            };

            let mut ci = _self.state.write().await;
            ci.peerid = pid.clone().to_uppercase();
            drop(ci);
            let (s, r) = async_std::channel::bounded::<()>(1);
            let zsession = _self.z.clone();
            let state = _self.state.clone();
            let path = format!(
                "zservice/Hello/2967c40b-a9a4-4330-b5f6-e0315b2356a9/{}/state",
                _self.instance_uuid()
            );

            let run_loop =
                async move {
                    let mut queryable = zsession.declare_queryable(&path).res().await?;
                    let kexpr: KeyExpr = (path.clone().try_into())
                        .map_err(|e| zrpc::zrpcresult::ZRPCError::ZenohError(format!("{e:?}")))?;

                    loop {
                        let query = queryable
                            .recv_async()
                            .await
                            .map_err(|_| zrpc::zrpcresult::ZRPCError::MissingValue)?;

                        let ci = state.read().await;
                        let data = zrpc::serialize::serialize_state(&*ci)?;
                        drop(ci);
                        let value = zenoh::prelude::Value::new(data.into())
                            .encoding(zenoh::prelude::Encoding::APP_OCTET_STREAM);

                        let sample = zenoh::prelude::Sample::new(kexpr.clone(), value);
                        query.reply(Ok(sample)).res().await.map_err(|e| {
                            zrpc::zrpcresult::ZRPCError::ZenohError(format!("{e:?}"))
                        })?;
                    }
                };

            let (abort_handle, abort_registration) = zrpc::AbortHandle::new_pair();

            let task_handle =
                async_std::task::spawn(zrpc::Abortable::new(run_loop, abort_registration));

            Ok((abort_handle, task_handle))
        }
        Box::pin(__connect(self))
    }
    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn initialize(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __initialize<S>(_self: &ServeHello<S>) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            let mut ci = _self.state.write().await;
            match ci.status {
                zrpc::ComponentStatus::HALTED => {
                    ci.status = zrpc::ComponentStatus::INITIALIZING;
                    Ok(())
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "Cannot initialize a component in a state different than HALTED".to_string(),
                )),
            }
        }
        Box::pin(__initialize(self))
    }
    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn register(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __register<S>(_self: &ServeHello<S>) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            let mut ci = _self.state.write().await;
            match ci.status {
                zrpc::ComponentStatus::INITIALIZING => {
                    ci.status = zrpc::ComponentStatus::REGISTERED;
                    Ok(())
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "Cannot register a component in a state different than INITIALIZING"
                        .to_string(),
                )),
            }
        }
        Box::pin(__register(self))
    }
    #[allow(
        clippy::type_complexity,
        clippy::manual_async_fn,
        clippy::needless_question_mark
    )]
    fn start(
        &self,
    ) -> ::core::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = ZRPCResult<(
                        zrpc::AbortHandle,
                        async_std::task::JoinHandle<Result<ZRPCResult<()>, zrpc::Aborted>>,
                    )>,
                > + '_,
        >,
    > {
        async fn __start<S>(
            _self: &ServeHello<S>,
        ) -> ZRPCResult<(
            zrpc::AbortHandle,
            async_std::task::JoinHandle<Result<ZRPCResult<()>, zrpc::Aborted>>,
        )>
        where
            S: Hello + Send + 'static,
        {
            let barrier = async_std::sync::Arc::new(async_std::sync::Barrier::new(2));
            let ci = _self.state.read().await;
            match ci.status {
                zrpc::ComponentStatus::REGISTERED => {
                    drop(ci);

                    let server = _self.clone();
                    let b = barrier.clone();

                    let (abort_handle, abort_registration) = zrpc::AbortHandle::new_pair();

                    let task_handle = async_std::task::spawn_blocking(move || {
                        async_std::task::block_on(zrpc::Abortable::new(
                            async { server.serve(b).await },
                            abort_registration,
                        ))
                    });

                    barrier.wait().await;

                    let mut ci = _self.state.write().await;
                    ci.status = zrpc::ComponentStatus::SERVING;
                    drop(ci);

                    Ok((abort_handle, task_handle))
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "Cannot start a component in a state different than REGISTERED".to_string(),
                )),
            }
        }
        Box::pin(__start(self))
    }

    fn run(&self) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __run<S>(_self: &ServeHello<S>) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            use std::convert::TryInto;
            use zenoh::prelude::r#async::*;

            let path = format!(
                "zservice/Hello/2967c40b-a9a4-4330-b5f6-e0315b2356a9/{}/eval",
                _self.instance_uuid()
            );

            let queryable = _self.z.declare_queryable(&path).res().await?;

            let kexpr: KeyExpr = (path.clone().try_into())
                .map_err(|e| zrpc::zrpcresult::ZRPCError::ZenohError(format!("{e:?}")))?;

            log::trace!("Declared queryable on: {:?}", path);
            loop {
                let query = queryable
                    .recv_async()
                    .await
                    .map_err(|_| zrpc::zrpcresult::ZRPCError::MissingValue)?;

                log::debug!("Received query {:?}", query);
                let selector = query.selector();
                match query.value() {
                    Some(value) => {
                        let req = zrpc::serialize::deserialize_request::<HelloRequest>(
                            &value.payload.contiguous(),
                        )?;

                        let mut ser = _self.server.clone();
                        let encoded_resp = match req {
                            HelloRequest::Hello { name } => {
                                let resp = HelloResponse::Hello(ser.hello(name).await);
                                zrpc::serialize::serialize_response(&resp)
                            }
                            HelloRequest::Add {} => {
                                let resp = HelloResponse::Add(ser.add().await);
                                zrpc::serialize::serialize_response(&resp)
                            }
                        }?;
                        let value =
                            Value::new(encoded_resp.into()).encoding(Encoding::APP_OCTET_STREAM);
                        let sample = Sample::new(kexpr.clone(), value);

                        query.reply(Ok(sample)).res().await.map_err(|e| {
                            zrpc::zrpcresult::ZRPCError::ZenohError(format!("{e:?}"))
                        })?;
                    }
                    None => log::error!(
                        "Received query on {:?} without value, not replying!",
                        selector
                    ),
                }
            }
        }

        Box::pin(__run(self))
    }

    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn serve(
        &self,
        barrier: async_std::sync::Arc<async_std::sync::Barrier>,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __serve<S>(
            _self: &ServeHello<S>,
            _barrier: async_std::sync::Arc<async_std::sync::Barrier>,
        ) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            let ci = _self.state.read().await;
            match ci.status {
                zrpc::ComponentStatus::REGISTERED => {
                    drop(ci);
                    _barrier.wait().await;

                    loop {
                        match _self.run().await {
                            Err(e) => {
                                log::error!("The run loop existed with {e:?}, restaring...");
                            }
                            Ok(_) => {
                                log::warn!("The run loop existed with unit restaring...");
                            }
                        }
                    }
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "State is not WORK, serve called directly? serve is called by calling work!"
                        .to_string(),
                )),
            }
        }
        let res = __serve(self, barrier);
        Box::pin(res)
    }

    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn stop(
        &self,
        stop: zrpc::AbortHandle,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __stop<S>(_self: &ServeHello<S>, _stop: zrpc::AbortHandle) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            let mut ci = _self.state.write().await;
            match ci.status {
                zrpc::ComponentStatus::SERVING => {
                    ci.status = zrpc::ComponentStatus::REGISTERED;
                    drop(ci);
                    _stop.abort();
                    Ok(())
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "Cannot stop a component in a state different than WORK".to_string(),
                )),
            }
        }
        Box::pin(__stop(self, stop))
    }
    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn unregister(
        &self,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __unregister<S>(_self: &ServeHello<S>) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            let mut ci = _self.state.write().await;
            match ci.status {
                zrpc::ComponentStatus::REGISTERED => {
                    ci.status = zrpc::ComponentStatus::HALTED;
                    Ok(())
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "Cannot unregister a component in a state different than REGISTERED"
                        .to_string(),
                )),
            }
        }
        Box::pin(__unregister(self))
    }
    #[allow(clippy::type_complexity, clippy::manual_async_fn)]
    fn disconnect(
        &self,
        stop: zrpc::AbortHandle,
    ) -> ::core::pin::Pin<Box<dyn std::future::Future<Output = ZRPCResult<()>> + '_>> {
        async fn __disconnect<S>(_self: &ServeHello<S>, _stop: zrpc::AbortHandle) -> ZRPCResult<()>
        where
            S: Hello + Send + 'static,
        {
            let mut ci = _self.state.write().await;
            match ci.status {
                zrpc::ComponentStatus::HALTED => {
                    ci.status = zrpc::ComponentStatus::HALTED;
                    drop(ci);
                    _stop.abort();
                    Ok(())
                }
                _ => Err(ZRPCError::StateTransitionNotAllowed(
                    "Cannot disconnect a component in a state different than HALTED".to_string(),
                )),
            }
        }
        Box::pin(__disconnect(self, stop))
    }
}

/// The request sent over the wire from the client to the server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HelloRequest {
    Hello { name: String },
    Add {},
}

/// The response sent over the wire from the server to the client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HelloResponse {
    Hello(String),
    Add(u64),
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub struct HelloClient<C = zrpc::ZClientChannel<HelloRequest, HelloResponse>> {
    ch: C,
    server_uuid: Uuid,
}

impl HelloClient {
    pub fn new(z: async_std::sync::Arc<zenoh::Session>, instance_id: uuid::Uuid) -> HelloClient {
        let new_client = zrpc::ZClientChannel::new(
            z,
            "zservice/Hello/2967c40b-a9a4-4330-b5f6-e0315b2356a9/".to_string(),
            Some(instance_id),
        );
        HelloClient {
            ch: new_client,
            server_uuid: instance_id,
        }
    }
    pub fn get_server_uuid(&self) -> Uuid {
        self.server_uuid
    }
    pub fn find_servers(
        z: async_std::sync::Arc<Session>,
    ) -> impl std::future::Future<Output = ZRPCResult<Vec<uuid::Uuid>>> + 'static {
        async move {
            use zenoh::prelude::r#async::*;

            let selector =
                "zservice/Hello/2967c40b-a9a4-4330-b5f6-e0315b2356a9/*/state".to_string();
            let mut servers = Vec::new();
            let replies = z.get(&selector).target(QueryTarget::All).res().await?;

            while let Ok(d) = replies.recv_async().await {
                match d.sample {
                    Ok(sample) => match sample.value.encoding {
                        Encoding::APP_OCTET_STREAM => {
                            let ca = zrpc::serialize::deserialize_state::<zrpc::ComponentState>(
                                &sample.value.payload.contiguous(),
                            )?;
                            servers.push(ca.uuid);
                        }
                        _ => {
                            return Err(ZRPCError::ZenohError(
                                "Server information is not correctly encoded".to_string(),
                            ))
                        }
                    },
                    Err(e) => {
                        return Err(ZRPCError::ZenohError(format!(
                            "Unable to get sample from {e:?}"
                        )))
                    }
                }
            }
            Ok(servers)
        }
    }

    pub fn find_servers_info(
        z: async_std::sync::Arc<Session>,
    ) -> impl std::future::Future<Output = ZRPCResult<Vec<zrpc::ComponentState>>> + 'static {
        async move {
            use zenoh::prelude::r#async::*;

            let selector =
                "zservice/Hello/2967c40b-a9a4-4330-b5f6-e0315b2356a9/*/state".to_string();
            let mut servers = Vec::new();
            let replies = z.get(&selector).target(QueryTarget::All).res().await?;

            while let Ok(d) = replies.recv_async().await {
                match d.sample {
                    Ok(sample) => match sample.value.encoding {
                        Encoding::APP_OCTET_STREAM => {
                            let ca = zrpc::serialize::deserialize_state::<zrpc::ComponentState>(
                                &sample.value.payload.contiguous(),
                            )?;
                            servers.push(ca);
                        }
                        _ => {
                            return Err(ZRPCError::ZenohError(
                                "Server information is not correctly encoded".to_string(),
                            ))
                        }
                    },
                    Err(e) => {
                        return Err(ZRPCError::ZenohError(format!(
                            "Unable to get sample from {e:?}"
                        )))
                    }
                }
            }
            Ok(servers)
        }
    }
    pub fn find_local_servers(
        z: async_std::sync::Arc<zenoh::Session>,
    ) -> impl std::future::Future<Output = ZRPCResult<Vec<uuid::Uuid>>> + 'static {
        async move {
            use zenoh::prelude::r#async::*;
            use zenoh::query::*;

            let servers = Self::find_servers_info(async_std::sync::Arc::clone(&z)).await?;

            let zinfo = z.info();

            let rid = match zinfo
                .routers_zid()
                .res()
                .await
                .collect::<Vec<ZenohId>>()
                .first()
            {
                Some(head) => head.to_string().to_uppercase(),
                None => "".to_string(),
            };
            if rid.is_empty() {
                return Ok(vec![]);
            }

            let selector = format!("@/router/{}", rid);
            let mut rdata: Vec<Reply> = z.get(&selector).res().await?.into_iter().collect();

            if rdata.is_empty() {
                return Err(ZRPCError::NotFound);
            }
            let router_data = rdata.remove(0);
            match router_data.sample {
                Ok(sample) => match sample.value.encoding {
                    Encoding::APP_JSON => {
                        let ri = zrpc::serialize::deserialize_router_info(
                            &sample.value.payload.contiguous(),
                        )?;
                        let r: Vec<Uuid> = servers
                            .into_iter()
                            .filter_map(|ci| {
                                let pid = String::from(&ci.peerid).to_uppercase();
                                let mut it = ri.clone().sessions.into_iter();
                                let f = it.find(|x| x.peer == pid.clone());
                                if f.is_none() {
                                    None
                                } else {
                                    Some(ci.uuid)
                                }
                            })
                            .collect();

                        Ok(r)
                    }
                    _ => Err(ZRPCError::ZenohError(
                        "Router information is not encoded in JSON".to_string(),
                    )),
                },
                Err(e) => Err(ZRPCError::ZenohError(format!(
                    "Unable to get sample from {e:?}"
                ))),
            }
        }
    }
}
impl HelloClient {
    pub fn verify_server(&self) -> impl std::future::Future<Output = ZRPCResult<bool>> + '_ {
        async move { self.ch.verify_server().await }
    }
    #[allow(unused, clippy::manual_async_fn)]
    pub fn hello(
        &self,
        name: String,
    ) -> impl std::future::Future<Output = ZRPCResult<String>> + '_ {
        let request = HelloRequest::Hello { name };
        async move {
            let resp = self.ch.call_fun(request);
            let dur = std::time::Duration::from_secs(60u16 as u64);
            match async_std::future::timeout(dur, resp).await {
                Ok(r) => match r {
                    Ok(zr) => match zr {
                        HelloResponse::Hello(msg) => std::result::Result::Ok(msg),
                        _ => Err(ZRPCError::Unreachable),
                    },
                    Err(e) => Err(e),
                },
                Err(e) => Err(ZRPCError::TimedOut),
            }
        }
    }
    #[allow(unused, clippy::manual_async_fn)]
    pub fn add(&self) -> impl std::future::Future<Output = ZRPCResult<u64>> + '_ {
        let request = HelloRequest::Add {};
        async move {
            let resp = self.ch.call_fun(request);
            let dur = std::time::Duration::from_secs(60u16 as u64);
            match async_std::future::timeout(dur, resp).await {
                Ok(r) => match r {
                    Ok(zr) => match zr {
                        HelloResponse::Add(msg) => std::result::Result::Ok(msg),
                        _ => Err(ZRPCError::Unreachable),
                    },
                    Err(e) => Err(e),
                },
                Err(e) => Err(ZRPCError::TimedOut),
            }
        }
    }
}

#[derive(Clone, Debug)]
struct HelloZService {
    pub ser_name: String,
    pub counter: Arc<Mutex<u64>>,
}

impl Hello for HelloZService {
    #[allow(unused, clippy::manual_async_fn)]
    fn hello(
        &self,
        name: String,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = String> + core::marker::Send + '_>>
    {
        async fn __hello(_self: &HelloZService, name: String) -> String {
            {
                format!("Hello {}!, you are connected to {}", name, _self.ser_name)
            }
        }
        Box::pin(__hello(self, name))
    }
    #[allow(unused, clippy::manual_async_fn)]
    fn add(
        &mut self,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = u64> + core::marker::Send + '_>>
    {
        async fn __add(mut _self: &HelloZService) -> u64 {
            let mut guard = _self.counter.lock().await;
            *guard += 1;
            *guard
        }
        Box::pin(__add(self))
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

        let service = HelloZService {
            ser_name: "test service".to_string(),
            counter: Arc::new(Mutex::new(0u64)),
        };

        let z = zsession.clone();
        let server = service.get_hello_server(z, None);

        let ser_uuid = server.instance_uuid();
        println!("Server instance UUID {}", ser_uuid);
        let client = HelloClient::new(zsession.clone(), ser_uuid);

        let (stopper, _h) = server.connect().await.unwrap();

        server.initialize().await.unwrap();
        server.register().await.unwrap();
        println!("Verify server: {:?}", client.verify_server().await);

        let (s, handle) = server.start().await.unwrap();

        let servers = HelloClient::find_servers(zsession.clone()).await;
        println!("servers found: {:?}", servers);

        let local_servers = HelloClient::find_local_servers(zsession.clone()).await;

        println!("local_servers found: {:?}", local_servers);

        println!("Verify server: {:?}", client.verify_server().await);
        task::sleep(Duration::from_secs(1)).await;

        println!("Verify server: {:?}", client.verify_server().await);

        let hello = client.hello("client".to_string()).await;
        println!("Res is: {:?}", hello);
        let res = client.add().await;
        println!("Res is: {:?}", res);

        let res = client.add().await;
        println!("Res is: {:?}", res);

        let res = client.add().await;
        println!("Res is: {:?}", res);

        server.stop(s).await.unwrap();
        server.unregister().await.unwrap();
        server.disconnect(stopper).await.unwrap();
        let _ = handle.await;
    }
}
