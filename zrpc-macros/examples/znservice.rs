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

#![allow(clippy::manual_async_fn)]
#![allow(clippy::large_enum_variant)]

#[macro_use]
extern crate std;

use async_std::sync::{Arc, Mutex};
use async_std::task;

use std::str;
use std::time::Duration;
use uuid::Uuid;

//importing the macros
use zrpc::zrpcresult::{ZRPCError, ZRPCResult};
use zrpc::ZServe;
use zrpc_macros::{znserver, znservice};

#[znservice(timeout_s = 60, prefix = "/lfos")]
pub trait Hello {
    async fn hello(&self, name: String) -> String;
    async fn add(&mut self) -> u64;
}

#[derive(Clone)]
struct HelloZService {
    pub ser_name: String,
    pub counter: Arc<Mutex<u64>>,
}

#[znserver]
impl Hello for HelloZService {
    async fn hello(&self, name: String) -> String {
        format!("Hello {}!, you are connected to {}", name, self.ser_name)
    }

    async fn add(&mut self) -> u64 {
        let mut guard = self.counter.lock().await;
        *guard += 1;
        *guard
    }
}

#[async_std::main]
async fn main() {
    env_logger::init();
    let mut config = zenoh::config::Config::default();
    config
        .set_mode(Some(zenoh::config::whatami::WhatAmI::Peer))
        .unwrap();
    let zsession = Arc::new(zenoh::open(config).await.unwrap());

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

    // let servers = HelloClient::find_servers(zsession.clone()).await;
    // println!("servers found: {:?}", servers);

    // this should return an error as the server is not ready
    // let hello = client.hello("client".to_string()).await;
    // println!("Res is: {:?}", hello);

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

    // let servers = HelloClient::find_servers(zsession.clone()).await;
    // println!("servers found: {:?}", servers);

    server.unregister().await.unwrap();
    server.disconnect(stopper).await.unwrap();

    let _ = handle.await;

    // this should return an error as the server is not there
    // let hello = client.hello("client".to_string()).await;
    // println!("Res is: {:?}", hello);
}
