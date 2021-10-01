use futures::prelude::*;
use structopt::StructOpt;
use zenoh::{prelude::*, queryable::EVAL};

static DEFAULT_MODE: &str = "peer";
static DEFAULT_SIZE: &str = "8";

#[derive(StructOpt, Debug)]
struct GetArgs {
    /// Config file
    #[structopt(short, long, default_value = DEFAULT_MODE)]
    mode: String,
    #[structopt(short, long)]
    peer: Option<String>,
    #[structopt(short, long, default_value = DEFAULT_SIZE)]
    size: u64,
}

#[async_std::main]
async fn main() {
    let args = GetArgs::from_args();

    let properties = match args.peer {
        Some(peer) => format!("mode={};peer={}", args.mode, peer),
        None => format!("mode={}", args.mode),
    };
    let zproperties = Properties::from(properties);
    let zenoh = zenoh::open(zproperties).await.unwrap();

    let path = String::from("/test/eval");

    let data: Vec<u8> = vec![0; args.size as usize];
    let mut query_stream = zenoh.register_queryable(&path).kind(EVAL).await.unwrap();
    while let Some(query) = query_stream.receiver().next().await {
        let value = Value::new(data.clone().into());
        let sample = Sample::new(path.clone(), value);
        query.reply_async(sample).await;
    }
}
