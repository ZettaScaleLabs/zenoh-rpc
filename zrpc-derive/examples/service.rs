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
use async_trait::async_trait;
use zrpc::prelude::*;

use std::str;

use zenoh::prelude::r#async::*;
use zrpc_derive::service;

#[service(timeout_s = 60)]
pub trait Hello {
    async fn hello(&self, name: String) -> String;
    async fn add(&mut self) -> u64;
    async fn test_serde_json_value(&self, value: serde_json::Value) -> bool;
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

    async fn test_serde_json_value(
        &self,
        request: Request<TestSerdeJsonValueRequest>,
    ) -> Result<Response<TestSerdeJsonValueResponse>, Status> {
        let value = &request.get_ref().value;
        match value {
            serde_json::Value::Bool(b) => Ok(Response::new((*b).into())),
            _ => Ok(Response::new(false.into())),
        }
    }
}

#[async_std::main]
async fn main() {
    env_logger::init();
    use zenoh::prelude::r#async::*;

    let mut config = zenoh::config::Config::default();
    config
        .set_mode(Some(zenoh::config::whatami::WhatAmI::Peer))
        .unwrap();
    let zsession = Arc::new(zenoh::open(config).res().await.unwrap());

    let z = zsession.clone();
    let client = HelloClient::new(zsession).await;

    task::spawn(async move {
        let service = MyServer {
            ser_name: "test service".to_string(),
            counter: Arc::new(Mutex::new(0u64)),
        };
        let mut server = zrpc::prelude::Server::new(z);
        server.add_service(Arc::new(HelloServer::new(service)));
        server.serve().await;
    });

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

    // press_to_continue().await;
    // let res = client.sub(Request::new(SubRequest {})).await;
    // println!("Res is: {:?}", res);
    let req = TestSerdeJsonValueRequest {
        value: serde_json::Value::Bool(true),
    };
    let res = client.test_serde_json_value(Request::new(req)).await;
    println!("Res is: {:?}", res);
}

async fn press_to_continue() {
    println!("Press ENTER to continue...");
    let buffer = &mut [0u8];
    async_std::io::stdin().read_exact(buffer).await.unwrap();
}
