//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::channel::unbounded;
use async_std::stream::StreamExt;
use async_std::sync::{Arc, Barrier, Mutex};
use async_std::task;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use zenoh::prelude::r#async::*;

static DEFAULT_MODE: &str = "peer";
static DEFAULT_SIZE: &str = "8";
static DEFAULT_INT: &str = "1";
static DEFAULT_DURATION: &str = "60";
#[derive(StructOpt, Debug)]
struct PingArgs {
    /// Zenoh mode, client or peer
    #[structopt(short, long, default_value = DEFAULT_MODE)]
    mode: String,
    #[structopt(short, long)]
    peer: Option<String>,
    #[structopt(short, long, default_value = DEFAULT_SIZE)]
    size: u64,
    #[structopt(short, long, default_value = DEFAULT_INT)]
    interveal: f64,
    #[structopt(short, long, default_value = DEFAULT_DURATION)]
    duration: u64,
}

type PingInfo = (u64, usize, u128);

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let args = PingArgs::from_args();

    let scenario = if args.mode == "peer" {
        "PP-PING"
    } else {
        "CRC-PING"
    };

    let mut config = zenoh::config::Config::default();
    config.set_mode(Some(args.mode.parse().unwrap())).unwrap();

    match args.peer {
        Some(peer) => {
            config.connect.endpoints.extend(vec![peer.parse().unwrap()]);
        }
        None => (),
    };
    let session = Arc::new(zenoh::open(config).res().await.unwrap());

    let (s, r) = unbounded::<PingInfo>();

    // The hashmap with the pings
    let pending = Arc::new(Mutex::new(HashMap::<u64, Instant>::new()));
    let barrier = Arc::new(Barrier::new(2));

    let c_pending = pending.clone();
    let c_barrier = barrier.clone();
    let c_session = session.clone();
    task::spawn(async move {
        // The resource to wait the response back
        let reskey_pong = String::from("test/pong");

        let sub = c_session
            .declare_subscriber(&reskey_pong)
            .res()
            .await
            .unwrap();

        // Wait for the both publishers and subscribers to be declared
        c_barrier.wait().await;
        println!("SQ_NUMBER,SIZE,RTT_US,SCENARIO");
        while let Ok(mut sample) = sub.recv_async().await {
            let mut count_bytes = [0u8; 8];
            sample.value.payload.read_bytes(&mut count_bytes);
            let count = u64::from_le_bytes(count_bytes);
            let instant = c_pending.lock().await.remove(&count).unwrap();
            s.send((
                count,
                sample.value.payload.len(),
                instant.elapsed().as_micros(),
            ))
            .await
            .unwrap();
            //print!("{},{},{},{}\n", count,sample.payload.len(),instant.elapsed().as_micros(),scenario);
        }
    });

    task::spawn(async move {
        loop {
            while let Ok(pi) = r.recv().await {
                let (c, s, rtt) = pi;
                print!("{},{},{},{}\n", c, s, rtt, scenario);
            }
        }
    });

    let d = args.duration;
    task::spawn(async move {
        task::sleep(Duration::from_secs(d)).await;
        std::process::exit(0);
    });

    // The resource to publish data on
    let reskey_ping = String::from("test/ping");

    // Wait for the both publishers and subscribers to be declared
    barrier.wait().await;

    let payload = vec![0u8; args.size as usize - 8];
    let mut count: u64 = 0;
    let i = args.interveal;
    loop {
        let mut data: WBuf = WBuf::new(args.size as usize, true);
        let count_bytes: [u8; 8] = count.to_le_bytes();
        data.write_bytes(&count_bytes);
        data.write_bytes(&payload);

        let value = Value::new(data.into());

        pending.lock().await.insert(count, Instant::now());
        session
            .put(&reskey_ping, value)
            .congestion_control(zenoh::publication::CongestionControl::Block) // Make sure to not drop messages because of congestion control
            .await
            .unwrap();

        task::sleep(Duration::from_secs_f64(i)).await;
        count += 1;
    }
}
