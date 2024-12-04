use log::*;
use std::sync::Arc;
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::tungstenite::Error;

use crate::{
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    server::{connection::handle_connection, server_state::ServerState},
};

pub async fn run_aggregator_server() -> CrateResult<()> {
    let server_state = Arc::new(Mutex::new(ServerState::new()?));
    let websocket_server = spawn_websocket_server(server_state.clone());
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

pub fn spawn_websocket_server(
    server_state: Arc<Mutex<ServerState>>,
) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{}", WEBSOCKET_PORT);
        let listener = TcpListener::bind(&addr).await?;
        info!("Listening on: {}", addr);

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

fn spawn_block_producer(server_state: Arc<Mutex<ServerState>>) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            let mut server_state = server_state.lock().await;

            // Start collecting signatures, only if there are transactions
            // The method returns None if there are no transactions
            match server_state.start_collecing_signatures().await {
                Ok(value) => {
                    if value.is_none() {
                        continue;
                    }
                }
                Err(e) => {
                    error!("Error collecting signatures: {}", e);

                    continue;
                }
            }

            // Wait for clients to send signatures
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            server_state.finalise().await;
        }
    })
}
