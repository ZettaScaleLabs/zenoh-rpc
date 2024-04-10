//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use async_std::task::sleep;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use zenoh::config::Config;
// use zenoh::payload::Serialize;
use zenoh::prelude::r#async::*;
use zenoh_typed::prelude::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MyData {
    pub name: String,
    pub id: u64,
}

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, value, _attachment) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();
    let session = SerdeSession::new(session, CBOR);

    println!("Declaring Publisher on '{key_expr}'...");
    // let publisher = session.declare_publisher(&key_expr).res().await.unwrap();

    for idx in 0..u32::MAX {
        sleep(Duration::from_secs(1)).await;
        let value: MyData = MyData {
            name: value.clone(),
            id: idx as u64,
        };
        println!("Putting Data ('{key_expr}': '{value:?}')...");

        session.put(&key_expr, value).res().await.unwrap();

        // TypedSession::<CBOREncoder, CBOREncoder>::put(&session, &key_expr, &value)
        //     .res()
        //     .await
        //     .unwrap();
    }
}

#[derive(Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-pub")]
    /// The key expression to write to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Pub from Rust!")]
    /// The value to write.
    value: String,
    #[arg(short, long)]
    /// The attachments to add to each put.
    ///
    /// The key-value pairs are &-separated, and = serves as the separator between key and value.
    attach: Option<String>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, String, Option<String>) {
    let args = Args::parse();
    (args.common.into(), args.key, args.value, args.attach)
}

#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Wai {
    Peer,
    Client,
    Router,
}
impl core::fmt::Display for Wai {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        core::fmt::Debug::fmt(&self, f)
    }
}
#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CommonArgs {
    #[arg(short, long)]
    /// A configuration file.
    config: Option<String>,
    #[arg(short, long)]
    /// The Zenoh session mode [default: peer].
    mode: Option<Wai>,
    #[arg(short = 'e', long)]
    /// Endpoints to connect to.
    connect: Vec<String>,
    #[arg(short, long)]
    /// Endpoints to listen on.
    listen: Vec<String>,
    #[arg(long)]
    /// Disable the multicast-based scouting mechanism.
    no_multicast_scouting: bool,
    #[arg(long)]
    /// Disable the multicast-based scouting mechanism.
    enable_shm: bool,
}

impl From<CommonArgs> for Config {
    fn from(value: CommonArgs) -> Self {
        (&value).into()
    }
}
impl From<&CommonArgs> for Config {
    fn from(value: &CommonArgs) -> Self {
        let mut config = match &value.config {
            Some(path) => Config::from_file(path).unwrap(),
            None => Config::default(),
        };
        match value.mode {
            Some(Wai::Peer) => config.set_mode(Some(zenoh::scouting::WhatAmI::Peer)),
            Some(Wai::Client) => config.set_mode(Some(zenoh::scouting::WhatAmI::Client)),
            Some(Wai::Router) => config.set_mode(Some(zenoh::scouting::WhatAmI::Router)),
            None => Ok(None),
        }
        .unwrap();
        if !value.connect.is_empty() {
            config.connect.endpoints = value.connect.iter().map(|v| v.parse().unwrap()).collect();
        }
        if !value.listen.is_empty() {
            config.listen.endpoints = value.listen.iter().map(|v| v.parse().unwrap()).collect();
        }
        if value.no_multicast_scouting {
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
        }
        if value.enable_shm {
            #[cfg(feature = "shared-memory")]
            config.transport.shared_memory.set_enabled(true).unwrap();
            #[cfg(not(feature = "shared-memory"))]
            {
                println!("enable-shm argument: SHM cannot be enabled, because Zenoh is compiled without shared-memory feature!");
                std::process::exit(-1);
            }
        }
        config
    }
}
