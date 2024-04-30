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

use std::collections::HashSet;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use zenoh::liveliness::LivelinessToken;
use zenoh::prelude::r#async::*;

use zenoh::Session;

use crate::serialize::serialize;
use crate::service::Service;
use crate::status::{Code, Status};
use crate::types::{Message, ServerMetadata, ServerTaskFuture, WireMessage};

pub struct ServerBuilder {
    pub(crate) session: Arc<Session>,
    pub(crate) services: HashMap<String, Arc<dyn Service + Send + Sync>>,
    pub(crate) labels: HashSet<String>,
}

impl ServerBuilder {
    pub fn session(mut self, session: Arc<Session>) -> Self {
        self.session = session;
        self
    }

    pub fn add_service(mut self, svc: Arc<dyn Service + Send + Sync>) -> Self {
        self.services.insert(svc.name(), svc);
        self
    }

    pub fn services(mut self, services: HashMap<String, Arc<dyn Service + Send + Sync>>) -> Self {
        self.services = services;
        self
    }

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

    pub fn build(self) -> Server {
        Server {
            session: self.session,
            services: self.services,
            tokens: Arc::new(Mutex::new(Vec::new())),
            labels: self.labels,
        }
    }
}

pub struct Server {
    pub(crate) session: Arc<Session>,
    pub(crate) services: HashMap<String, Arc<dyn Service + Send + Sync>>,
    pub(crate) tokens: Arc<Mutex<Vec<LivelinessToken<'static>>>>,
    pub(crate) labels: HashSet<String>,
}

impl Server {
    pub fn builder(session: Arc<Session>) -> ServerBuilder {
        ServerBuilder {
            session,
            services: HashMap::new(),
            labels: HashSet::new(),
        }
    }

    pub fn instance_uuid(&self) -> ZenohId {
        self.session.zid()
    }

    // this should return a stopper paramerer,
    // I.e, the Sender side of a channel.
    pub async fn serve(&self) -> Result<(), Status> {
        let mut tokens = vec![];
        // register the queryables and declare a liveliness token
        let ke = format!("@rpc/{}/**", self.instance_uuid());

        let queryable = self
            .session
            .declare_queryable(&ke)
            .res()
            .await
            .map_err(|e| {
                Status::new(
                    Code::InternalError,
                    format!("Cannot declare queryable: {e:?}"),
                )
            })?;

        for k in self.services.keys() {
            let ke = format!("@rpc/{}/service/{k}", self.instance_uuid());
            let lt = self
                .session
                .liveliness()
                .declare_token(ke)
                .res()
                .await
                .map_err(|e| {
                    Status::new(
                        Code::InternalError,
                        format!("Cannot declare liveliness token: {e:?}"),
                    )
                })?;
            tokens.push(lt)
        }

        self.tokens.lock().await.extend(tokens);

        loop {
            let query = queryable
                .recv_async()
                .await
                .map_err(|e| Status::internal_error(format!("Cannot receive query: {e:?}")))?;

            // the query for RPC is is in the format: @rpc/<server id>/service/<service-name>/<method-name>
            // everything is sent as payload of the query
            // in the future metadata and method name could be sent as attachments.

            // for the PaaS we need to know which region the server is responsible of
            // thus the idea is to add labels to the server, that can be used for querying the
            // network when looking for servers
            // the query in this case will be @rpc/<server-id>/metadata
            // and the labels are sent as payload of the reply
            // the caller then checks the metadata

            let ke = query.key_expr().clone();

            let fut: ServerTaskFuture = match Self::get_token(&ke, 2) {
                Some("service") => {
                    let service_name = Self::get_service_name(&ke)?;
                    let svc = self
                        .services
                        .get(service_name)
                        .ok_or_else(|| {
                            Status::internal_error(format!("Service not found: {service_name}"))
                        })?
                        .clone();

                    let payload = query
                        .value()
                        .ok_or_else(|| {
                            Status::internal_error("Query has empty value cannot proceed")
                        })?
                        .payload
                        .contiguous()
                        .to_vec();

                    // this is call to a service
                    Box::pin(Self::service_call(svc, ke.clone(), payload))
                }
                Some("metadata") => Box::pin(Self::server_metadata(
                    self.labels.clone(),
                    self.instance_uuid(),
                )),
                Some(_) | None => {
                    // this calls returns internal error
                    Box::pin(Self::create_error())
                }
            };
            // This could easility become a task pool with a channel where the futures
            // are submitted fox execution
            tokio::task::spawn(async move {
                let res = fut.await;
                let sample = match res {
                    Ok(data) => Sample::new(ke, data),
                    Err(e) => {
                        let wmgs = WireMessage {
                            payload: None,
                            status: e,
                        };
                        Sample::new(ke, serialize(&wmgs).unwrap_or_default())
                    }
                };
                let res = query.reply(Ok(sample)).res().await;
                log::trace!("Query Result is: {res:?}");
            });
        }

        // Ok(())
    }

    async fn service_call(
        svc: Arc<dyn Service + Send + Sync>,
        ke: KeyExpr<'_>,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let method_name = Self::get_method_name(&ke)?;

        let msg = Message {
            method: method_name.into(),
            body: payload,
            metadata: HashMap::new(),
            status: Status::new(Code::Accepted, ""),
        };

        match svc.call(msg).await {
            Ok(msg) => {
                log::trace!("Service response: {msg:?}");
                let wmsg = WireMessage {
                    payload: Some(msg.body),
                    status: Status::ok(""),
                };

                serialize(&wmsg)
                    .map_err(|e| Status::internal_error(format!("Serialization error: {e:?}")))
            }
            Err(e) => {
                log::trace!("Service error is : {e:?}");
                let wmsg = WireMessage {
                    payload: None,
                    status: e,
                };
                serialize(&wmsg)
                    .map_err(|e| Status::internal_error(format!("Serialization error: {e:?}")))
            }
        }
    }

    async fn server_metadata(labels: HashSet<String>, id: ZenohId) -> Result<Vec<u8>, Status> {
        let metadata = ServerMetadata { labels, id };
        let serialized_metadata = serialize(&metadata)
            .map_err(|e| Status::internal_error(format!("Serialization error: {e:?}")))?;

        let wmsg = WireMessage {
            payload: Some(serialized_metadata),
            status: Status::ok(""),
        };

        serialize(&wmsg).map_err(|e| Status::internal_error(format!("Serialization error: {e:?}")))
    }

    async fn create_error() -> Result<Vec<u8>, Status> {
        Err(Status::unavailable("Unavailable"))
    }

    fn get_service_name<'a>(ke: &'a KeyExpr) -> Result<&'a str, Status> {
        Self::get_token(ke, 3).ok_or(Status::internal_error("Cannot get service name"))
    }
    fn get_method_name<'a>(ke: &'a KeyExpr) -> Result<&'a str, Status> {
        Self::get_token(ke, 4).ok_or(Status::internal_error("Cannot get method name"))
    }

    fn get_token<'a>(ke: &'a KeyExpr, index: usize) -> Option<&'a str> {
        let tokens: Vec<_> = ke.split('/').collect();
        tokens.get(index).copied()
    }
}
