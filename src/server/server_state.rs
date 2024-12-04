use std::collections::HashMap;

use futures_util::{stream::SplitSink, SinkExt};
use log::{error, info, warn};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    aggregator::Aggregator,
    errors::CrateResult,
    rollup::mock_rollup_fs::MockRollupFS,
    types::{
        common::{BlsPublicKey, BlsSignature, U8_32},
        public_key::BlsPublicKeyWrapper,
    },
};

use super::ws_message::WsMessage;

pub struct Connection {
    pub public_key: BlsPublicKey,
    // To send messages to the client over their websocket connection
    pub ws_send: SplitSink<WebSocketStream<TcpStream>, Message>,
}

pub struct ServerState {
    connections: HashMap<BlsPublicKeyWrapper, Connection>,
    // Indexes which connections have transactions, the value is initially false when they send a transaction and then set to true when they send a signature
    connections_with_tx: HashMap<BlsPublicKeyWrapper, bool>,
    aggregator: Aggregator,
    rollup_state: MockRollupFS,
}

impl ServerState {
    pub fn new() -> CrateResult<ServerState> {
        Ok(ServerState {
            connections: HashMap::new(),
            aggregator: Aggregator::new(),
            connections_with_tx: HashMap::new(),
            rollup_state: MockRollupFS::new()?,
        })
    }

    pub fn add_connection(&mut self, connection: Connection) {
        self.connections
            .insert(connection.public_key.clone().into(), connection);
    }

    pub fn remove_connection(&mut self, public_key: &BlsPublicKey) {
        self.connections.remove(&public_key.into());
    }

    pub async fn start_collecing_signatures(&mut self) -> CrateResult<Option<()>> {
        if self.aggregator.tx_hash_to_metadata.len() == 0 {
            info!("No transactions to start collecting signatures for");
            return Ok(None);
        }

        // Validates that there are transactions to collect signatures for
        self.aggregator.start_collecting_signatures()?;

        // TODO: Send inclusion proofs to all the clients

        info!("Starting to collect signatures");
        for (connection, _) in self.connections_with_tx.iter() {
            match self.connections.get_mut(connection) {
                // TODO: Needs to send the inclusion proof to the user
                Some(connection) => {
                    if let Err(e) = connection
                        .ws_send
                        .send(WsMessage::SStartCollectingSignatures.into())
                        .await
                    {
                        error!(
                            "Failed to send start collecting signatures message: {:?}",
                            e
                        );
                    }
                }
                None => {
                    warn!("Connection not found for public key: {:?}", connection);
                }
            }
        }

        Ok(Some(()))
    }

    pub fn add_batch(&mut self, tx_hash: &U8_32, public_key: &BlsPublicKey) -> CrateResult<()> {
        self.aggregator.add_batch(tx_hash, public_key)?;

        self.connections_with_tx
            .insert(public_key.clone().into(), false);

        Ok(())
    }

    pub fn add_signature(
        &mut self,
        tx_hash: &U8_32,
        public_key: &BlsPublicKey,
        signature: &BlsSignature,
    ) -> CrateResult<()> {
        // This checks for the existence of the transaction and public key
        self.aggregator
            .add_signature(tx_hash, public_key, signature)?;

        self.connections_with_tx
            .insert(public_key.clone().into(), true);

        Ok(())
    }

    pub async fn finalise(&mut self) {
        info!("Finalising aggregator");

        // Finalise and message all the connections
        // aggregator.finalise does a variety of checks to ensure the aggregator is in the correct state
        match self.aggregator.finalise() {
            Ok(transfer_block) => {
                for connection in self.connections.values_mut() {
                    if let Err(e) = connection
                        .ws_send
                        .send(WsMessage::SFinalised(transfer_block.clone()).into())
                        .await
                    {
                        error!("Failed to send finalise message: {:?}", e);
                    }
                }
            }
            Err(e) => {
                error!("Error finalising aggregator: {}", e);
            }
        }

        // Create a new aggregator now we have finalised
        self.aggregator = Aggregator::new();
    }
}
