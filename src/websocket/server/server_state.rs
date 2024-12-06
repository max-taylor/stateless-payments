use std::collections::HashMap;

use futures_util::{stream::SplitSink, SinkExt};
use log::{error, info, warn};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    aggregator::Aggregator,
    errors::CrateResult,
    rollup::{mock_rollup_fs::MockRollupFS, traits::RollupStateTrait},
    types::{
        balance::BalanceProof,
        public_key::BlsPublicKeyWrapper,
        signatures::{BlsPublicKey, BlsSignature},
        transaction::{TransactionBatch, TransactionProof},
    },
    websocket::ws_message::WsMessage,
};

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
            return Ok(None);
        }

        // Validates that there are transactions to collect signatures for
        self.aggregator.start_collecting_signatures()?;

        info!("Starting to collect signatures");
        for (connection, _) in self.connections_with_tx.iter() {
            match self.connections.get_mut(connection) {
                Some(connection) => {
                    if let Ok(proof) = self
                        .aggregator
                        .generate_proof_for_pubkey(&connection.public_key)
                    {
                        if let Err(e) = connection
                            .ws_send
                            .send(WsMessage::SSendTransactionInclusionProof(proof).into())
                            .await
                        {
                            error!(
                                "Failed to send start collecting signatures message: {:?}",
                                e
                            );
                        }
                    } else {
                        error!(
                            "Failed to generate inclusion proof for public key: {:?}",
                            connection.public_key
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

    pub fn add_batch(&mut self, batch: &TransactionBatch) -> CrateResult<()> {
        info!(
            "Received transaction batch from: {:?}",
            serde_json::to_string(&batch.from)?,
        );

        self.aggregator.add_batch(batch)?;

        self.connections_with_tx
            .insert(batch.from.clone().into(), false);

        Ok(())
    }

    pub fn add_signature(
        &mut self,
        public_key: &BlsPublicKey,
        signature: &BlsSignature,
    ) -> CrateResult<()> {
        info!(
            "Received transaction batch signature from: {:?}",
            serde_json::to_string(&public_key)?,
        );

        // This checks for the existence of the transaction and public key
        self.aggregator.add_signature(public_key, signature)?;

        self.connections_with_tx
            .insert(public_key.clone().into(), true);

        Ok(())
    }

    pub async fn send_batch_to_receivers(
        &mut self,
        proof: &TransactionProof,
        balance_proof: &BalanceProof,
    ) -> CrateResult<()> {
        info!("Sending transaction to receiver");

        for transaction in proof.batch.transactions.iter() {
            let connection = self.connections.get_mut(&transaction.to.into());
            if connection.is_none() {
                warn!("Connection not found for public key: {:?}", transaction.to);
                continue;
            }
            let connection = connection.unwrap();

            if let Err(e) = connection
                .ws_send
                .send(WsMessage::SReceiveTransaction(proof.clone(), balance_proof.clone()).into())
                .await
            {
                // Don't propogate again so we can continue to send to other connections
                error!("Failed to send transaction to receiver: {:?}", e);
            }
        }

        Ok(())
    }

    pub async fn finalise(&mut self) -> CrateResult<()> {
        info!("Finalising aggregator");

        // Finalise and message all the connections
        // aggregator.finalise does a variety of checks to ensure the aggregator is in the correct state
        let transfer_block = self.aggregator.finalise()?;

        self.rollup_state
            .add_transfer_block(transfer_block.clone())?;

        for (connection, _) in self.connections_with_tx.iter() {
            match self.connections.get_mut(connection) {
                Some(connection) => {
                    if let Err(e) = connection
                        .ws_send
                        .send(WsMessage::SFinalised(transfer_block.clone()).into())
                        .await
                    {
                        // Don't propogate errors here, because we want to continue to send to other connections
                        error!("Failed to send finalise message: {:?}", e);
                    }
                }
                None => {
                    warn!("Connection not found for public key: {:?}", connection);
                }
            }
        }

        // Create a new aggregator now we have finalised
        self.aggregator = Aggregator::new();

        Ok(())
    }
}
