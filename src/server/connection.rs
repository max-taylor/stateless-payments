use anyhow::anyhow;
use futures_util::StreamExt;
use log::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpStream;
use tokio_tungstenite::accept_async;

use crate::{
    errors::CrateResult,
    server::{
        server_state::{Connection, ServerState},
        utils::parse_ws_message,
    },
    types::common::BlsPublicKey,
};

use super::ws_message::WsMessage;

struct ConnectionGuard {
    public_key: BlsPublicKey,
    server_state: Arc<ServerState>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        info!("Removing connection: {:?}", self.public_key);

        // Remove the connection from the server state, ignoring any errors
        let _ = self.server_state.remove_connection(&self.public_key);
    }
}

pub async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    server_state: Arc<ServerState>,
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
        info!("Received public key, adding connection: {:?}", public_key);

        let connection = Connection {
            public_key: public_key.clone(),
            ws_send: ws_sender,
        };
        _guard = ConnectionGuard {
            public_key: public_key.clone(),
            server_state: server_state.clone(),
        };

        let id = server_state.add_connection(connection).await;
    } else {
        return Err(anyhow!("Must send public key as first message"));
    }

    loop {
        if let Some(msg) = ws_receiver.next().await {
            let ws_message = parse_ws_message(msg?)?;

            match ws_message {
                WsMessage::CAddConnection(public_key) => {
                    error!(
                        "Received public key after initial message: {:?}",
                        public_key
                    );
                }
                WsMessage::CSendTransactionBatch(transaction_batch) => {
                    let mut aggregator = server_state.aggregator.lock().await;
                    aggregator.add_batch(&transaction_batch.tx_hash(), &transaction_batch.from)?;
                }
                WsMessage::CSendTransactionBatchSignature(tx_hash, from, signature) => {
                    let mut aggregator = server_state.aggregator.lock().await;
                    aggregator.add_signature(&tx_hash, &from, &signature)?;
                }

                _ => {
                    error!("Received unknown message: {:?}", ws_message);
                }
            }
        }
    }
}
