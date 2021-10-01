use structopt::StructOpt;
use zenoh::prelude::*;

static DEFAULT_MODE: &str = "peer";
static DEFAULT_SIZE: &str = "8";

#[derive(StructOpt, Debug)]
struct PutArgs {
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
    let args = PutArgs::from_args();

    //println!("Args {:?}", args);

    let properties = match args.peer {
        Some(peer) => format!("mode={};peer={}", args.mode, peer),
        None => format!("mode={}", args.mode),
    };
    let zproperties = Properties::from(properties);
    let zenoh = zenoh::open(zproperties).await.unwrap();

    let path = String::from("/test/thr");
    let data = vec![0; args.size as usize];
    let value = Value::new(data.into());

    loop {
        zenoh.put(&path, value.clone()).await.unwrap();
    }
}
