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

use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;

use async_trait::async_trait;
use zenoh::config::{EndPoint, ZenohId};
//importing the macros
use zrpc::prelude::*;
use zrpc_derive::service;

#[service(timeout_s = 60)]
pub trait Hello {
    async fn hello(&self, name: String) -> String;
    async fn add(&self) -> u64;
}

#[derive(Clone)]
struct MyServer {
    pub ser_name: String,
    pub counter: Arc<Mutex<u64>>,
}

#[async_trait]
impl Hello for MyServer {
    async fn hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        let name = &request.get_ref().name;
        Ok(Response::new(
            format!("Hello {}!, you are connected to {}", name, self.ser_name).into(),
        ))
    }

    async fn add(&self, _request: Request<AddRequest>) -> Result<Response<AddResponse>, Status> {
        let mut guard = self.counter.lock().await;
        *guard += 1;
        let res = *guard;
        Ok(Response::new(res.into()))
    }
}

fn configure_zenoh(id: ZenohId, listen: String, connect: String) -> zenoh::config::Config {
    let mut config = zenoh::config::Config::default();
    config.set_id(id).unwrap();
    config
        .set_mode(Some(zenoh::config::whatami::WhatAmI::Peer))
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();

    let listen: Vec<EndPoint> = vec![listen.parse().unwrap()];
    let connect: Vec<EndPoint> = vec![connect.parse().unwrap()];
    config.listen.endpoints.set(listen).unwrap();
    config.connect.endpoints.set(connect).unwrap();

    config
}

async fn wait_for_peer(session: &zenoh::Session, id: ZenohId) {
    while !session.info().peers_zid().await.any(|e| e == id) {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn service_call() {
    let server_zid = ZenohId::from_str("a0").unwrap();
    let client_zid = ZenohId::from_str("a1").unwrap();

    let client_config = configure_zenoh(
        client_zid,
        "tcp/127.0.0.1:9003".to_string(),
        "tcp/127.0.0.1:9002".to_string(),
    );

    let client_session = zenoh::open(client_config).await.unwrap();

    let c_zid_server = server_zid;
    tokio::task::spawn(async move {
        let server_config = configure_zenoh(
            c_zid_server,
            "tcp/127.0.0.1:9002".to_string(),
            "tcp/127.0.0.1:9003".to_string(),
        );
        let server_session = zenoh::open(server_config).await.unwrap();
        wait_for_peer(&server_session, client_zid).await;

        let service = MyServer {
            ser_name: "test service".to_string(),
            counter: Arc::new(Mutex::new(0u64)),
        };
        let builder = zrpc::prelude::Server::builder(server_session)
            .add_service(Arc::new(HelloServer::new(service)));

        let _ = builder.build().serve().await;
    });

    // Check zenoh sessions are connected

    wait_for_peer(&client_session, server_zid).await;

    //sleep 2s for KE propagation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let client = HelloClient::builder(client_session).build();

    let hello = client
        .hello(Request::new(HelloRequest {
            name: "client".to_string(),
        }))
        .await
        .unwrap();

    assert_eq!(
        String::from("Hello client!, you are connected to test service"),
        **hello.get_ref()
    );

    let res = client.add(Request::new(AddRequest {})).await.unwrap();

    assert_eq!(1, **res.get_ref());

    client.add(Request::new(AddRequest {})).await.unwrap();

    let res = client.add(Request::new(AddRequest {})).await.unwrap();

    assert_eq!(3, **res.get_ref());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn service_unavailable() {
    let server_zid = ZenohId::from_str("a2").unwrap();
    let client_zid = ZenohId::from_str("a3").unwrap();

    let server_config = configure_zenoh(
        server_zid,
        "tcp/127.0.0.1:9004".to_string(),
        "tcp/127.0.0.1:9005".to_string(),
    );

    let client_config = configure_zenoh(
        client_zid,
        "tcp/127.0.0.1:9005".to_string(),
        "tcp/127.0.0.1:9004".to_string(),
    );

    let server_session = zenoh::open(server_config).await.unwrap();
    let client_session = zenoh::open(client_config).await.unwrap();

    // Check zenoh sessions are connected
    wait_for_peer(&server_session, client_zid).await;
    wait_for_peer(&client_session, server_zid).await;

    let client = HelloClient::builder(client_session).build();
    //sleep 2s for KE propagation
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let res = client.add(Request::new(AddRequest {})).await;
    assert!(res.is_err())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn server_not_matching() {
    let server_zid = ZenohId::from_str("a4").unwrap();
    let client_zid = ZenohId::from_str("a5").unwrap();

    let client_config = configure_zenoh(
        client_zid,
        "tcp/127.0.0.1:9007".to_string(),
        "tcp/127.0.0.1:9006".to_string(),
    );

    let client_session = zenoh::open(client_config).await.unwrap();

    let c_zid_server = server_zid;
    let st = tokio::task::spawn(async move {
        let server_config = configure_zenoh(
            c_zid_server,
            "tcp/127.0.0.1:9006".to_string(),
            "tcp/127.0.0.1:9007".to_string(),
        );
        let server_session = zenoh::open(server_config).await.unwrap();
        wait_for_peer(&server_session, client_zid).await;

        let service = MyServer {
            ser_name: "test service".to_string(),
            counter: Arc::new(Mutex::new(0u64)),
        };
        let builder = zrpc::prelude::Server::builder(server_session)
            .add_label("test-1")
            .add_service(Arc::new(HelloServer::new(service)));

        let _ = builder.build().serve().await;
    });

    // Check zenoh sessions are connected

    wait_for_peer(&client_session, server_zid).await;

    //sleep 2s for KE propagation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let client = HelloClient::builder(client_session)
        .add_label("test-2")
        .build();

    let res = client.add(Request::new(AddRequest {})).await;
    assert!(res.is_err());
    st.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn server_matching() {
    let server_zid = ZenohId::from_str("a6").unwrap();
    let client_zid = ZenohId::from_str("a7").unwrap();

    let client_config = configure_zenoh(
        client_zid,
        "tcp/127.0.0.1:9009".to_string(),
        "tcp/127.0.0.1:9008".to_string(),
    );

    let client_session = zenoh::open(client_config).await.unwrap();

    let c_zid_server = server_zid;
    tokio::task::spawn(async move {
        let server_config = configure_zenoh(
            c_zid_server,
            "tcp/127.0.0.1:9008".to_string(),
            "tcp/127.0.0.1:9009".to_string(),
        );
        let server_session = zenoh::open(server_config).await.unwrap();
        wait_for_peer(&server_session, client_zid).await;

        let service = MyServer {
            ser_name: "test service".to_string(),
            counter: Arc::new(Mutex::new(0u64)),
        };
        let builder = zrpc::prelude::Server::builder(server_session)
            .add_label("test-1")
            .add_label("test-2")
            .add_service(Arc::new(HelloServer::new(service)));

        let _ = builder.build().serve().await;
    });

    // Check zenoh sessions are connected

    wait_for_peer(&client_session, server_zid).await;

    //sleep 2s for KE propagation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let client = HelloClient::builder(client_session)
        .add_label("test-2")
        .build();

    let res = client.add(Request::new(AddRequest {})).await;
    assert!(res.is_ok())
}
