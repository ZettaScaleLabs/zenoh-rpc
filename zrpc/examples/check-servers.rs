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
use tokio::io::AsyncReadExt;
use zenoh::config::ZenohId;
use zenoh::key_expr::format::KeFormat;

use zenoh::key_expr::KeyExpr;
use zrpc::prelude::*;

use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use zenoh::Session;

#[allow(unused)]
#[derive(Clone, Debug)]
pub struct ServerChecker {
    pub(crate) ch: RPCClientChannel,
    pub(crate) z: Session,
    pub(crate) tout: Duration,
    pub(crate) labels: HashSet<String>,
    pub(crate) service_name: String,
    kef: String,
}

// generated client code

impl ServerChecker {
    pub fn new(session: Session, service_name: String) -> Self {
        let kef = format!("@rpc/${{zid:*}}/service/{service_name}");
        Self {
            kef,
            ch: RPCClientChannel::new(session.clone(), &service_name),
            z: session,
            tout: std::time::Duration::from_secs(60u16 as u64),
            labels: HashSet::new(),
            service_name: service_name.clone(),
        }
    }

    async fn find_servers_livelines(&self) -> Result<Vec<ZenohId>, Status> {
        let res = self
            .z
            .liveliness()
            .get(format!("@rpc/*/service/{}", self.service_name))
            .await
            .map_err(|e| {
                Status::unavailable(format!("Unable to perform liveliness query: {e:?}"))
            })?;

        let ids = res
            .into_iter()
            .filter_map(|e| e.into_result().ok())
            .map(|e| self.extract_id_from_ke(e.key_expr()))
            .collect::<Result<Vec<ZenohId>, Status>>()?;

        Ok(ids)
    }

    async fn get_servers_metadata(&self) -> Result<Vec<ServerMetadata>, Status> {
        let ids = self.find_servers_livelines().await?;

        // get server metadata
        self.ch.get_servers_metadata(&ids, self.tout).await
    }

    // this could be improved by caching the server id
    // maybe by using this https://github.com/moka-rs/moka
    async fn find_servers(&self) -> Result<Vec<ZenohId>, Status> {
        // get server metadata
        let metadatas = self.get_servers_metadata().await?;

        // filter the metadata based on labels
        let ids: Vec<ZenohId> = metadatas
            .into_iter()
            .filter(|m| m.labels.is_superset(&self.labels))
            .map(|m| m.id)
            .collect();

        Ok(ids)
    }

    fn extract_id_from_ke(&self, ke: &KeyExpr) -> Result<ZenohId, Status> {
        let ke_format = KeFormat::new(&self.kef)
            .map_err(|e| Status::internal_error(format!("Cannot create KE format: {e:?}")))?;
        let id_str = ke_format
            .parse(ke)
            .map_err(|e| Status::internal_error(format!("Unable to parse key expression: {e:?}")))?
            .get("zid")
            .map_err(|e| {
                Status::internal_error(format!(
                    "Unable to get server id from key expression: {e:?}"
                ))
            })?;

        ZenohId::from_str(id_str)
            .map_err(|e| Status::internal_error(format!("Unable to convert str to ZenohId: {e:?}")))
    }
}

#[tokio::main]
async fn main() {
    {
        let config = zenoh::config::Config::from_file("./zenoh.json").unwrap();
        let zsession = zenoh::open(config).await.unwrap();

        let z = zsession.clone();

        let server_checker = ServerChecker::new(z, "K8sController".into());

        println!("Checking server liveliness");
        let res = server_checker.find_servers_livelines().await.unwrap();
        println!("Res is: {:?}", res);

        press_to_continue().await;
        println!("Checking server metadata");
        let res = server_checker.get_servers_metadata().await.unwrap();
        println!("Res is: {:?}", res);

        press_to_continue().await;
        println!("Checking server ids");
        let res = server_checker.find_servers().await.unwrap();
        println!("Res is: {:?}", res);

        press_to_continue().await;
    }
}

async fn press_to_continue() {
    println!("Press ENTER to continue...");
    let buffer = &mut [0u8];
    tokio::io::stdin().read_exact(buffer).await.unwrap();
}
