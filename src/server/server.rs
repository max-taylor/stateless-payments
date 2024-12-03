use log::*;
use std::sync::Arc;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_tungstenite::tungstenite::Error;

use crate::{
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    server::{connection::handle_connection, server_state::ServerState},
};

pub fn run_aggregator_server() -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async {
        let addr = format!("127.0.0.1:{}", WEBSOCKET_PORT);
        let listener = TcpListener::bind(&addr).await?;
        info!("Listening on: {}", addr);

        let server_state = Arc::new(ServerState::new());

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
