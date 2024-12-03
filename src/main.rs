#![allow(dead_code)]

use errors::CrateResult;
use server::server::run_aggregator_server;
use tokio;

mod aggregator;
mod wallet;
mod constants;
mod errors;
mod rollup;
mod server;
mod types;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let task = run_aggregator_server();

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let block_producer_thread_finished = task.is_finished();

        if block_producer_thread_finished {
            task.abort();

            let result = task.await;

            if let Err(e) = result {
                eprintln!("Error in block producer thread: {:?}", e);
            }
            break;
        }
    }

    Ok(())
}
