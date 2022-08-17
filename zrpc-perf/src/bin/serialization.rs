
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

use async_std::sync::Arc;
use async_std::task;
use std::str;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use structopt::StructOpt;
use uuid::Uuid;
use zrpc_macros::znservice;
use zrpc::zrpcresult::{ZRPCError, ZRPCResult};

static DEFAULT_INT: &str = "5";
static DEFAULT_SIZE: &str = "8";
static DEFAULT_DURATION: &str = "60";

#[derive(StructOpt, Debug)]
struct CallArgs {
    /// Config file
    #[structopt(short, long, default_value = DEFAULT_SIZE)]
    size: u64,
    #[structopt(short, long, default_value = DEFAULT_INT)]
    interveal: u64,
    #[structopt(short, long, default_value = DEFAULT_DURATION)]
    duration: u64,
}

#[znservice(
    timeout_s = 60,
    prefix = "/test",
    service_uuid = "00000000-0000-0000-0000-000000000001"
)]
pub trait Bench {
    async fn bench(&self) -> Vec<u8>;
}

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let args = CallArgs::from_args();
    let rtts = Arc::new(AtomicU64::new(0));
    let count: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    let kind = "ZNRPC-RESP-SER";

    let c = count.clone();
    let s = args.size;
    let i = args.interveal;
    let rt = rtts.clone();
    println!("MSGS,SIZE,THR,INTERVEAL,RTT_US,KIND");
    task::spawn(async move {
        loop {
            task::sleep(Duration::from_secs(i)).await;
            let n = c.swap(0, Ordering::AcqRel);
            let r = rt.swap(0, Ordering::AcqRel);
            let msgs = n / i;
            let thr = (n * s * 8) / i;
            let rtt = if n == 0 {
                0f64
            } else {
                (r as f64) / (n as f64)
            } as f64;
            println!("{},{},{},{},{},{}", msgs, s, thr, i, rtt, kind);
        }
    });

    let d = args.duration;
    task::spawn(async move {
        task::sleep(Duration::from_secs(d)).await;
        std::process::exit(0);
    });

    let data = vec![0; args.size as usize];
    let resp = BenchResponse::Bench(data);

    loop {
        let now_q = Instant::now();
        let _d = zrpc::serialize::serialize_response(&resp.clone()).unwrap();
        count.fetch_add(1, Ordering::AcqRel);
        rtts.fetch_add(now_q.elapsed().as_micros() as u64, Ordering::AcqRel);
    }
}
