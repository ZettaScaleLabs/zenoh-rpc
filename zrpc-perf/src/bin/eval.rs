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

    let mut config = zenoh::config::Config::default();
    config.set_mode(Some(args.mode.parse().unwrap())).unwrap();

    match args.peer {
        Some(peer) => {
            let peers: Vec<Locator> = vec![peer.parse().unwrap()];
            config.set_peers(peers).unwrap();
        }
        None => (),
    };

    let zenoh = zenoh::open(config).await.unwrap();

    let path = String::from("/test/eval");

    let data: Vec<u8> = vec![0; args.size as usize];
    let mut query_stream = zenoh.queryable(&path).kind(EVAL).await.unwrap();
    while let Some(query) = query_stream.receiver().next().await {
        let value = Value::new(data.clone().into());
        let sample = Sample::new(path.clone(), value);
        query.reply_async(sample).await;
    }
}
