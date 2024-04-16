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

use async_std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use zenoh::liveliness::LivelinessToken;
use zenoh::prelude::r#async::*;

use zenoh::Session;

use crate::serialize::serialize;
use crate::service::Service;
use crate::status::{Code, Status};
use crate::types::{Message, WireMessage};

pub struct Server {
    session: Arc<Session>,
    services: HashMap<String, Arc<dyn Service + Send + Sync>>,
    tokens: Arc<Mutex<Vec<LivelinessToken<'static>>>>,
}

impl Server {
    pub fn new(session: Arc<Session>) -> Self {
        Self {
            session,
            services: HashMap::new(),
            tokens: Arc::new(Mutex::new(vec![])),
        }
    }

    pub fn add_service(&mut self, svc: Arc<dyn Service + Send + Sync>) {
        self.services.insert(svc.name(), svc);
    }

    pub fn instance_uuid(&self) -> ZenohId {
        self.session.zid()
    }

    pub async fn serve(&self) {
        let mut tokens = vec![];
        // register the queryables and declare a liveliness token
        let ke = format!("@rpc/{}/**", self.instance_uuid());

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

        self.tokens.lock().await.extend(tokens);

        loop {
            let query = queryable.recv_async().await.unwrap();

            // the query for RPC is is in the format: @rpc/<server id>/service/<service-name>/<method-name>
            // everything is sent as payload of the query
            // in the future metadata and method name could be sent as attachments.

            // for the PaaS we need to know which region the server is responsible of
            // thus the idea is to add labels to the server, that can be used for querying the
            // network when looking for servers
            // the query in this case will be @rpc/<server-id>/metadata
            // and the labels are sent as payload of the reply
            // the caller then checks the metadata

            let svcs = self.services.clone();
            let c_ke = KeyExpr::try_from(ke.clone()).unwrap();
            async_std::task::spawn(async move {
                let ke = query.selector().key_expr.clone();

                // here we should get the name of the service and from it the name of the service
                // and the name of the method, we use the ke formatter

                let service_name = Self::get_service_name(&ke);
                let method_name = Self::get_method_name(&ke);
                let svc = svcs.get(service_name).unwrap();
                let payload: Vec<u8> = query.value().unwrap().payload.contiguous().to_vec();

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

                        serialize(&wmsg)
                    }
                    Err(e) => {
                        let wmsg = WireMessage {
                            payload: None,
                            status: e,
                        };
                        serialize(&wmsg)
                    }
                }
                .unwrap();
                let sample = Sample::new(c_ke, resp);
                query.reply(Ok(sample)).res().await.unwrap();
            });
        }
    }

    fn get_service_name<'a>(ke: &'a KeyExpr) -> &'a str {
        Self::get_token(ke, 3)
    }
    fn get_method_name<'a>(ke: &'a KeyExpr) -> &'a str {
        Self::get_token(ke, 4)
    }

    fn get_token<'a>(ke: &'a KeyExpr, index: usize) -> &'a str {
        let tokens: Vec<_> = ke.split('/').collect();
        tokens.get(index).unwrap()
    }
}
