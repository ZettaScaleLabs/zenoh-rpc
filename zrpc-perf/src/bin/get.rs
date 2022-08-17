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


use async_std::sync::Arc;
use async_std::task;
use futures::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use structopt::StructOpt;
use zenoh::prelude::*;

static DEFAULT_MODE: &str = "peer";
static DEFAULT_SIZE: &str = "8";
static DEFAULT_INT: &str = "5";
static DEFAULT_DURATION: &str = "60";

#[derive(StructOpt, Debug)]
struct GetArgs {
    /// Config file
    #[structopt(short, long, default_value = DEFAULT_MODE)]
    mode: String,
    #[structopt(short, long)]
    peer: Option<String>,
    #[structopt(short, long, default_value = DEFAULT_SIZE)]
    size: u64,
    #[structopt(short, long, default_value = DEFAULT_INT)]
    interveal: u64,
    #[structopt(short, long, default_value = DEFAULT_DURATION)]
    duration: u64,
}

#[async_std::main]
async fn main() {
    let args = GetArgs::from_args();

    let rtts = Arc::new(AtomicU64::new(0));
    let count: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    let mut config = zenoh::config::Config::default();
    config.set_mode(Some(args.mode.parse().unwrap())).unwrap();

    match args.peer {
        Some(peer) => {
            let peers: Vec<Locator> = vec![peer.clone().parse().unwrap()];
            config.set_peers(peers).unwrap();
        }
        None => (),
    };
    let zenoh = zenoh::open(config).await.unwrap();

    let selector = format!("/test/{}", args.size);

    let kind = if args.mode == "peer" {
        "PR-GET"
    } else {
        "CR-GET"
    };

    let path = format!("/test/{}", args.size);
    let data = vec![0; args.size as usize];
    let value = Value::new(data.into());
    zenoh.put(&path, value).await.unwrap();

    println!("MSGS,SIZE,THR,INTERVEAL,RTT_US,KIND");
    let c = count.clone();
    let s = args.size;
    let i = args.interveal;
    let rt = rtts.clone();
    task::spawn(async move {
        loop {
            task::sleep(Duration::from_secs(i)).await;
            let n = c.swap(0, Ordering::AcqRel);
            let r = rt.swap(0, Ordering::AcqRel);
            let msgs = n / i;
            let thr = (n * s * 8) / i;
            let rtt = if n == 0 { 0 } else { r / n };
            println!("{},{},{},{},{},{}", msgs, s, thr, i, rtt, kind);
        }
    });

    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(args.duration) {
        let now_q = Instant::now();
        let mut data_stream = zenoh.get(&selector).await.unwrap();
        while data_stream.next().await.is_some() {}
        count.fetch_add(1, Ordering::AcqRel);
        rtts.fetch_add(now_q.elapsed().as_micros() as u64, Ordering::AcqRel);
    }

    zenoh.close().await.unwrap();
}
