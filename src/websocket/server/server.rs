use log::*;
use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{constants::WEBSOCKET_PORT, errors::CrateResult, rollup::mock_rollup_fs::MockRollupFS};

use super::server_state::ServerState;

pub async fn run_aggregator_server() -> CrateResult<()> {
    let rollup_state = MockRollupFS::new()?;
    let (server_state, websocket_server, _) =
        ServerState::new_with_ws_server(rollup_state, Some(WEBSOCKET_PORT)).await?;
    let block_producer = spawn_block_producer(server_state.clone(), Some(10));

    // Combine the two tasks into one
    // This will allow us to return an error if either of the tasks fail
    let (websocket_result, block_producer_result) =
        tokio::try_join!(websocket_server, block_producer)?;

    if let Err(e) = websocket_result {
        error!("Websocket server error: {}", e);
    }

    if let Err(e) = block_producer_result {
        error!("Block producer error: {}", e);
    }

    Ok(())
}

pub fn spawn_block_producer(
    server_state: Arc<Mutex<ServerState>>,
    production_delay_seconds: Option<u64>,
) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                production_delay_seconds.unwrap_or(10),
            ))
            .await;

            println!("Starting block production");
            // Start collecting signatures, only if there are transactions
            // The method returns None if there are no transactions
            match server_state
                .lock()
                .await
                .start_collecting_signatures()
                .await
            {
                Ok(value) => {
                    if value.is_none() {
                        info!("No transactions to start collecting signatures for");
                        continue;
                    }
                }
                Err(e) => {
                    error!("Error collecting signatures: {}", e);

                    continue;
                }
            }

            info!("Waiting for clients to send signatures");
            // Wait for clients to send signatures
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            if let Err(e) = server_state.lock().await.finalise().await {
                error!("Error finalising: {}", e);
            }
        }
    })
}
