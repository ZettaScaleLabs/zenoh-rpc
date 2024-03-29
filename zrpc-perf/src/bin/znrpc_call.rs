#![allow(clippy::manual_async_fn)]
#![allow(clippy::large_enum_variant)]
#[macro_use]
extern crate std;

use async_std::sync::Arc;
use async_std::task;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use structopt::StructOpt;
use zenoh::prelude::r#async::*;

use std::str;
use uuid::Uuid;
use zrpc::zrpcresult::{ZRPCError, ZRPCResult};
use zrpc::ZServe;
use zrpc_macros::{zserver, zservice};

static DEFAULT_MODE: &str = "client";
static DEFAULT_ZMODE: &str = "peer";
static DEFAULT_ROUTER: &str = "tcp/127.0.0.1:7447";
static DEFAULT_INT: &str = "5";
static DEFAULT_SIZE: &str = "8";
static DEFAULT_DURATION: &str = "60";

#[derive(StructOpt, Debug)]
struct CallArgs {
    /// Config file
    #[structopt(short, long, default_value = DEFAULT_MODE)]
    mode: String,
    #[structopt(short, long, default_value = DEFAULT_ZMODE)]
    zenoh_mode: String,
    #[structopt(short, long, default_value = DEFAULT_ROUTER)]
    router: String,
    #[structopt(short, long, default_value = DEFAULT_SIZE)]
    size: u64,
    #[structopt(short, long, default_value = DEFAULT_INT)]
    interveal: u64,
    #[structopt(short, long, default_value = DEFAULT_DURATION)]
    duration: u64,
}

#[zservice(
    timeout_s = 60,
    prefix = "test",
    service_uuid = "00000000-0000-0000-0000-000000000001"
)]
pub trait Bench {
    async fn bench(&self) -> Vec<u8>;
}

#[derive(Clone)]
struct BenchZService {
    pub data: Vec<u8>,
}

#[zserver]
impl Bench for BenchZService {
    async fn bench(&self) -> Vec<u8> {
        self.data.clone()
    }
}

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let args = CallArgs::from_args();

    if args.mode == "server" {
        server(args).await;
    } else if args.mode == "client" {
        client(args).await;
    } else {
        panic!("Mode can be only one of [client|server]")
    }
}

async fn client(args: CallArgs) {
    let rtts = Arc::new(AtomicU64::new(0));
    let count: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    let mut config = zenoh::config::Config::default();
    config
        .set_mode(Some(args.zenoh_mode.parse().unwrap()))
        .unwrap();

    if args.zenoh_mode == "client" {
        config
            .connect
            .endpoints
            .extend(vec![args.router.parse().unwrap()]);
    }
    let zenoh = Arc::new(zenoh::open(config).res().await.unwrap());

    task::sleep(std::time::Duration::from_secs(1)).await;

    let local_servers = BenchClient::find_servers(zenoh.clone()).await.unwrap();
    log::trace!("Servers: {:?}", local_servers);

    let client = BenchClient::new(zenoh.clone(), local_servers[0]);

    let kind = if args.zenoh_mode == "client" {
        "CRC-ZRPC"
    } else {
        "PP-ZRPC"
    };

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
            let rtt = if n == 0 { 0 } else { r / n };
            println!("{},{},{},{},{},{}", msgs, s, thr, i, rtt, kind);
        }
    });

    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(args.duration) {
        let now_q = Instant::now();
        let _r = client.bench().await;
        count.fetch_add(1, Ordering::AcqRel);
        rtts.fetch_add(now_q.elapsed().as_micros() as u64, Ordering::AcqRel);
    }

    //zenoh.close().await.unwrap();
}

async fn server(args: CallArgs) {
    let mut config = zenoh::config::Config::default();
    config
        .set_mode(Some(args.zenoh_mode.parse().unwrap()))
        .unwrap();

    if args.zenoh_mode == "client" {
        config
            .connect
            .endpoints
            .extend(vec![args.router.parse().unwrap()]);
    }

    let zenoh = Arc::new(zenoh::open(config).res().await.unwrap());

    let data = vec![0; args.size as usize];

    let service = BenchZService { data };

    let server = service.get_bench_server(zenoh.clone(), None);

    println!("Instance ID {}", server.instance_uuid());
    let (_ss, _h) = server.connect().await.unwrap();
    server.initialize().await.unwrap();
    server.register().await.unwrap();
    let (_s, handle) = server.start().await.unwrap();

    let _ = handle.await;
}
