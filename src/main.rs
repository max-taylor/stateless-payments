#![allow(dead_code)]

use errors::StatelessBitcoinResult;
use server::server::run_aggregator_server;
use tokio;

mod aggregator;
mod client;
mod errors;
mod rollup;
mod server;
mod types;

#[tokio::main]
async fn main() -> StatelessBitcoinResult<()> {
    let server = run_aggregator_server();

    Ok(())
}
