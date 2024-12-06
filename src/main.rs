#![allow(dead_code)]

use errors::CrateResult;
use tokio;
use websocket::server::server::run_aggregator_server;

mod aggregator;
mod constants;
mod errors;
mod rollup;
mod types;
mod wallet;
mod websocket;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let task = run_aggregator_server().await;

    // loop {
    //     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    //
    //     let block_producer_thread_finished = task.is_finished();
    //
    //     if block_producer_thread_finished {
    //         task.abort();
    //
    //         let result = task.await;
    //
    //         if let Err(e) = result {
    //             eprintln!("Error in block producer thread: {:?}", e);
    //         }
    //         break;
    //     }
    // }

    Ok(())
}
