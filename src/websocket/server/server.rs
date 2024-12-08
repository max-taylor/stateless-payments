use log::*;
use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinHandle};

use crate::errors::CrateResult;

use super::server_state::ServerState;

pub async fn run_aggregator_server() -> CrateResult<()> {
    let (server_state, websocket_server) = ServerState::new_with_ws_server().await?;
    let block_producer = spawn_block_producer(server_state.clone());

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

fn spawn_block_producer(server_state: Arc<Mutex<ServerState>>) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            // Start collecting signatures, only if there are transactions
            // The method returns None if there are no transactions
            match server_state.lock().await.start_collecing_signatures().await {
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
