use log::*;
use std::sync::Arc;
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::tungstenite::Error;

use crate::{
    aggregator::AggregatorState,
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    rollup::mock_rollup_fs::MockRollupFS,
    server::{connection::handle_connection, server_state::ServerState},
};

pub fn run_aggregator_server() -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async {
        let addr = format!("127.0.0.1:{}", WEBSOCKET_PORT);
        let listener = TcpListener::bind(&addr).await?;
        info!("Listening on: {}", addr);

        let server_state = Arc::new(Mutex::new(ServerState::new()?));

        loop {
            let listener_value = listener.accept().await;

            let server_state = server_state.clone();

            if let Err(e) = listener_value {
                error!("Error accepting connection: {}", e);
                continue;
            }

            let (stream, socket_addr) = listener_value?;

            tokio::spawn(async move {
                if let Err(e) = handle_connection(socket_addr, stream, server_state).await {
                    let custom_error = e.downcast_ref::<Error>();
                    match custom_error {
                        Some(Error::ConnectionClosed) => {
                            info!("Connection closed: {}", socket_addr);
                        }
                        _ => error!("Error handling connection: {}", e),
                    }
                }
            });
        }
    })
}

fn block_producer_task(server_state: Arc<Mutex<ServerState>>) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            let mut server_state = server_state.lock().await;

            // if server_state.aggregator.tx_hash_to_metadata.len() == 0
            //     || server_state.aggregator.state != AggregatorState::Open
            // {
            //     continue;
            // }

            info!("Collecting signatures");

            if let Err(e) = server_state.start_collecing_signatures().await {
                error!("Error collecting signatures: {}", e);
            }

            // Wait for clients to send signatures
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
    })
}
