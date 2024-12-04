use std::collections::HashMap;

use futures_util::stream::SplitSink;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    aggregator::Aggregator,
    errors::CrateResult,
    types::{common::BlsPublicKey, public_key::BlsPublicKeyWrapper},
};

pub struct Connection {
    pub public_key: BlsPublicKey,
    // To send messages to the client over their websocket connection
    pub ws_send: SplitSink<WebSocketStream<TcpStream>, Message>,
}

pub struct ServerState {
    connections: Mutex<HashMap<BlsPublicKeyWrapper, Connection>>,
    pub aggregator: Mutex<Aggregator>,
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            connections: Mutex::new(HashMap::new()),
            aggregator: Mutex::new(Aggregator::new()),
        }
    }

    pub async fn add_connection(&self, connection: Connection) -> CrateResult<()> {
        self.connections
            .lock()
            .await
            .insert(connection.public_key.clone().into(), connection);

        Ok(())
    }

    // ! Commenting out for now
    // pub async fn upsert_pubkey_to_id(&self, public_key: BlsPublicKey) -> CrateResult<u32> {
    //     let mut pubkey_to_id = self.pubkey_to_id.lock().await;
    //     let value = pubkey_to_id.get(&public_key.into());
    //     let length: u32 = pubkey_to_id.len().try_into()?;
    //
    //     if value.is_none() {
    //         pubkey_to_id.insert(public_key.clone().into(), length);
    //
    //         return Ok(length);
    //     }
    //
    //     let value = value.ok_or(anyhow!("Error upserting pubkey to id"))?;
    //
    //     return Ok(*value);
    // }

    // pub async fn find_pubkey_to_id(&self, id: u32) -> CrateResult<BlsPublicKey> {
    //     let pubkey_to_id = self.pubkey_to_id.lock().await;
    //     let value = pubkey_to_id.iter().find(|(_, v)| **v == id);
    //
    //     let value = value.ok_or(anyhow!("Error finding pubkey to id"))?;
    //
    //     return Ok(value.0.clone().into());
    // }

    pub async fn remove_connection(&self, public_key: &BlsPublicKey) {
        self.connections.lock().await.remove(&public_key.into());
    }
    //
    // pub async fn test_send(&self, public_key: &BlsPublicKeyWrapper, message: Message) {
    //     if let Some(connection) = self.connections.lock().await.get_mut(public_key) {
    //         connection.ws_send.send(message).await.unwrap();
    //     }
    // }
}
