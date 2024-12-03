use std::collections::HashMap;

use futures_util::{stream::SplitSink, SinkExt};
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    aggregator::Aggregator,
    types::{common::BlsPublicKey, public_key::BlsPublicKeyWrapper},
};

// pub enum Message {
//     Data(Vec<u8>),
//     Close,
// }

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

    pub async fn add_connection(&self, connection: Connection) {
        self.connections
            .lock()
            .await
            .insert(connection.public_key.clone().into(), connection);
    }

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
