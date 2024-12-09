use anyhow::anyhow;
use futures_util::StreamExt;
use log::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::{self, JoinHandle},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    types::signatures::BlsPublicKey,
    websocket::{
        server::server_state::Connection,
        ws_message::{parse_ws_message, WsMessage},
    },
};

use super::server_state::ServerState;

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
                    let custom_error = e.downcast_ref::<tokio_tungstenite::tungstenite::Error>();
                    match custom_error {
                        Some(tokio_tungstenite::tungstenite::Error::ConnectionClosed) => {
                            info!("Connection closed: {}", socket_addr);
                        }
                        _ => error!("Error handling connection: {}", e),
                    }
                }
            });
        }
    })
}

struct ConnectionGuard {
    public_key: BlsPublicKey,
    server_state: Arc<Mutex<ServerState>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let server_state = self.server_state.clone();
        let public_key = self.public_key.clone();
        task::spawn(async move {
            println!("Dropping connection: {:?}", public_key);
            // Perform the cleanup asynchronously
            let mut state = server_state.lock().await;
            println!("Removing connection: {:?}", public_key);
            state.remove_connection(&public_key).await
        });
    }
}

pub async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    server_state: Arc<Mutex<ServerState>>,
) -> CrateResult<()> {
    let ws_stream = accept_async(stream).await.expect("Failed to accept");
    info!("New WebSocket connection: {}", peer);
    let (ws_sender, mut ws_receiver) = ws_stream.split();

    let msg = ws_receiver
        .next()
        .await
        .ok_or(anyhow!("Must send public key as first message"))?;

    // Declare the guard here so that it is dropped when the function returns, which will remove the connection
    let _guard: ConnectionGuard;

    if let WsMessage::CAddConnection(public_key) = parse_ws_message(msg?)? {
        info!(
            "Received public key, adding connection: {:?}",
            serde_json::to_string(&public_key)?
        );

        let connection = Connection {
            public_key: public_key.clone(),
            ws_send: ws_sender,
        };
        _guard = ConnectionGuard {
            public_key: public_key.clone(),
            server_state: server_state.clone(),
        };

        server_state.lock().await.add_connection(connection);
    } else {
        return Err(anyhow!("Must send public key as first message"));
    }

    loop {
        if let Some(msg) = ws_receiver.next().await {
            // Intentionally ignore errors here, as we don't want to drop the connection
            if let Err(e) = handle_loop(msg, server_state.clone()).await {
                error!("Error handling message: {:?}", e);
            }
        } else {
            return Err(tokio_tungstenite::tungstenite::Error::ConnectionClosed.into());
        }
    }
}

async fn handle_loop(
    msg: Result<Message, tokio_tungstenite::tungstenite::Error>,
    server_state: Arc<Mutex<ServerState>>,
) -> CrateResult<()> {
    let ws_message = parse_ws_message(msg?)?;

    match ws_message {
        WsMessage::CSendTransactionBatch(transaction_batch) => {
            server_state.lock().await.add_batch(&transaction_batch)?;
        }
        WsMessage::CSendTransactionBatchSignature(from, signature) => {
            server_state.lock().await.add_signature(&from, &signature)?;
        }
        WsMessage::CSendBatchToReceivers(proof, balance_proof) => {
            server_state
                .lock()
                .await
                .send_batch_to_receivers(&proof, &balance_proof)
                .await?;
        }
        _ => {
            return Err(anyhow!("Invalid message type"));
        }
    }

    Ok(())
}
