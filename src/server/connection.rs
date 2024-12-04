use anyhow::anyhow;
use futures_util::StreamExt;
use log::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, sync::Mutex, task};
use tokio_tungstenite::{accept_async, tungstenite::Message};

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
    server_state: Arc<Mutex<ServerState>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let server_state = self.server_state.clone();
        let public_key = self.public_key.clone();
        task::spawn(async move {
            // Perform the cleanup asynchronously
            let mut state = server_state.lock().await;
            state.remove_connection(&public_key);
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
        info!("Received public key, adding connection: {:?}", public_key);

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
        WsMessage::CAddConnection(public_key) => {
            error!(
                "Received public key after initial message: {:?}",
                public_key
            );
        }
        WsMessage::CSendTransactionBatch(transaction_batch) => {
            info!(
                "Received transaction batch from: {:?}",
                serde_json::to_string(&transaction_batch.from)?,
            );
            server_state
                .lock()
                .await
                .add_batch(&transaction_batch.tx_hash(), &transaction_batch.from);
        }
        WsMessage::CSendTransactionBatchSignature(tx_hash, from, signature) => {
            info!(
                "Received transaction batch signature from: {:?}",
                serde_json::to_string(&from)?,
            );
            server_state
                .lock()
                .await
                .add_signature(&tx_hash, &from, &signature)?;
        }
        _ => {
            return Err(anyhow!("Invalid message type"));
        }
    }

    Ok(())
}
