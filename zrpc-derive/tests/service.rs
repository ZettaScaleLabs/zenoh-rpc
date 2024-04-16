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

use async_std::sync::{Arc, Mutex};

use async_trait::async_trait;
//importing the macros
use zenoh::prelude::r#async::*;
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
    config.listen.endpoints.push(listen.parse().unwrap());
    config.connect.endpoints.push(connect.parse().unwrap());

    config
}

async fn wait_for_peer(session: &zenoh::Session, id: ZenohId) {
    while !session.info().peers_zid().res().await.any(|e| e == id) {
        async_std::task::sleep(std::time::Duration::from_secs(1)).await
    }
}

#[test]
fn service_discovery() {
    // async_std::task::block_on(async {
    //     let server_zid = ZenohId::rand();
    //     let client_zid = ZenohId::rand();

    //     let server_config = configure_zenoh(
    //         server_zid,
    //         "tcp/127.0.0.1:9000".to_string(),
    //         "tcp/127.0.0.1:9001".to_string(),
    //     );

    //     let client_config = configure_zenoh(
    //         client_zid,
    //         "tcp/127.0.0.1:9001".to_string(),
    //         "tcp/127.0.0.1:9000".to_string(),
    //     );

    //     let server_session = Arc::new(zenoh::open(server_config).res().await.unwrap());
    //     let client_session = Arc::new(zenoh::open(client_config).res().await.unwrap());

    //     // Check zenoh sessions are connected
    //     wait_for_peer(&server_session, client_zid).await;
    //     wait_for_peer(&client_session, server_zid).await;

    //     let service = HelloZService {
    //         ser_name: "test service".to_string(),
    //         counter: Arc::new(Mutex::new(0u64)),
    //     };

    //     let server = service.get_hello_server(server_session, None);
    //     let ser_uuid = server.instance_uuid();
    //     let (stopper, _h) = server.connect().await.unwrap();
    //     server.initialize().await.unwrap();
    //     server.register().await.unwrap();
    //     let (s, handle) = server.start().await.unwrap();

    //     //sleep 1s for KE propagation
    //     async_std::task::sleep(std::time::Duration::from_secs(1)).await;

    //     let mut servers = HelloClient::find_servers(client_session).await.unwrap();
    //     assert_eq!(ser_uuid, servers.remove(0));

    //     server.stop(s).await.unwrap();
    //     server.unregister().await.unwrap();
    //     server.disconnect(stopper).await.unwrap();

    //     let _ = handle.await;
    // });
}

#[test]
fn service_call() {
    async_std::task::block_on(async {
        let server_zid = ZenohId::rand();
        let client_zid = ZenohId::rand();

        let client_config = configure_zenoh(
            client_zid,
            "tcp/127.0.0.1:9003".to_string(),
            "tcp/127.0.0.1:9002".to_string(),
        );

        let client_session = Arc::new(zenoh::open(client_config).res().await.unwrap());

        let c_zid_server = server_zid;
        async_std::task::spawn(async move {
            let server_config = configure_zenoh(
                c_zid_server,
                "tcp/127.0.0.1:9002".to_string(),
                "tcp/127.0.0.1:9003".to_string(),
            );
            let server_session = Arc::new(zenoh::open(server_config).res().await.unwrap());
            wait_for_peer(&server_session, client_zid).await;

            let service = MyServer {
                ser_name: "test service".to_string(),
                counter: Arc::new(Mutex::new(0u64)),
            };

            let mut server = Server::new(server_session);
            server.add_service(Arc::new(HelloServer::new(service)));

            server.serve().await;
        });

        // Check zenoh sessions are connected

        wait_for_peer(&client_session, server_zid).await;

        //sleep 1s for KE propagation
        async_std::task::sleep(std::time::Duration::from_secs(2)).await;

        let client = HelloClient::new(client_session).await;

        let hello = client
            .hello(Request::new(HelloRequest {
                name: "client".to_string(),
            }))
            .await
            .unwrap();

        assert_eq!(
            String::from("Hello client!, you are connected to test service"),
            hello.get_ref().0
        );

        let res = client.add(Request::new(AddRequest {})).await.unwrap();

        assert_eq!(1, res.get_ref().0);

        client.add(Request::new(AddRequest {})).await.unwrap();

        let res = client.add(Request::new(AddRequest {})).await.unwrap();

        assert_eq!(3, res.get_ref().0);
    });
}

#[test]
fn service_unavailable() {
    async_std::task::block_on(async {
        let server_zid = ZenohId::rand();
        let client_zid = ZenohId::rand();

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

        let server_session = Arc::new(zenoh::open(server_config).res().await.unwrap());
        let client_session = Arc::new(zenoh::open(client_config).res().await.unwrap());

        // Check zenoh sessions are connected
        wait_for_peer(&server_session, client_zid).await;
        wait_for_peer(&client_session, server_zid).await;

        let client = HelloClient::new(client_session).await;

        //sleep 1s for KE propagation
        async_std::task::sleep(std::time::Duration::from_secs(1)).await;

        let res = client.add(Request::new(AddRequest {})).await;
        assert!(res.is_err())
    });
}
