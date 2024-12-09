use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use futures_util::{sink::Close, stream::SplitSink, SinkExt};
use log::{error, info, warn};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
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

use super::connection::spawn_websocket_server;

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

    pub async fn new_with_ws_server(
    ) -> CrateResult<(Arc<Mutex<ServerState>>, JoinHandle<CrateResult<()>>)> {
        let server_state = Arc::new(Mutex::new(ServerState::new()?));
        let websocket_server = spawn_websocket_server(server_state.clone());
        Ok((server_state, websocket_server))
    }

    pub fn add_connection(&mut self, connection: Connection) {
        self.connections
            .insert(connection.public_key.clone().into(), connection);
    }

    pub async fn remove_connection(&mut self, public_key: &BlsPublicKey) -> CrateResult<()> {
        match self.connections.get_mut(&public_key.into()) {
            Some(connection) => {
                connection.ws_send.close().await?;
                self.connections.remove(&public_key.into());
            }
            None => {
                println!("No connection with tx for public key: {:?}", public_key);
            }
        };

        Ok(())
    }

    pub async fn start_collecting_signatures(&mut self) -> CrateResult<Option<()>> {
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
            .add_transfer_block(transfer_block.clone())
            .await?;

        // TODO: Will be removed
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

        self.connections_with_tx.clear();

        // Create a new aggregator now we have finalised
        self.aggregator = Aggregator::new();

        Ok(())
    }
}

// This is used in testing so that we can have a single instance of the server and prevent multiple instances from being created
static SERVER_INSTANCE: OnceLock<(Arc<Mutex<ServerState>>, JoinHandle<CrateResult<()>>)> =
    OnceLock::new();

// Singleton Server Implementation
pub struct SingletonServer;

impl SingletonServer {
    pub async fn get_instance(
    ) -> CrateResult<&'static (Arc<Mutex<ServerState>>, JoinHandle<CrateResult<()>>)> {
        match SERVER_INSTANCE.get() {
            Some(instance) => return Ok(instance),
            None => {
                let result = ServerState::new_with_ws_server().await?;

                return Ok(SERVER_INSTANCE.get_or_init(|| result));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use crate::{
        errors::CrateResult, rollup::mock_rollup_memory::MockRollupMemory, wallet::wallet::Wallet,
        websocket::client::client::Client,
    };

    use super::{ServerState, SingletonServer};

    async fn setup() -> CrateResult<(
        Arc<Mutex<ServerState>>,
        Arc<Mutex<Client>>,
        Arc<Mutex<MockRollupMemory>>,
    )> {
        let (server, _) = SingletonServer::get_instance().await?;
        // Delay 1s to allow the server to start
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let rollup_state = Arc::new(Mutex::new(MockRollupMemory::new()));
        let (client, _, _) = Client::new(Wallet::new(None), rollup_state.clone()).await?;

        Ok((server.clone(), client, rollup_state))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_connection_is_added() -> CrateResult<()> {
        let (server, client, _) = setup().await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let public_key = client.lock().await.wallet.public_key.clone();
        assert_eq!(
            server
                .lock()
                .await
                .connections
                .get(&public_key.into())
                .is_some(),
            true
        );

        client.lock().await.shutdown().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_add_batch() -> CrateResult<()> {
        // Test the batch is addeed to the aggregator
        // Test the connection is added to the connections_with_tx
        Ok(())
    }

    #[tokio::test]
    async fn test_add_signature() -> CrateResult<()> {
        // Test the signature is added to the aggregator
        // Test the connection is updated in the connections_with_tx
        Ok(())
    }

    #[tokio::test]
    async fn test_finalise() -> CrateResult<()> {
        // Test the transfer block is added to rollup state
        // Test connection_with_tx is cleared
        // Test aggregator is reset
        Ok(())
    }

    // TODO: End to end test
    #[tokio::test]
    async fn test_start_collecting_signatures_gets_signatures_from_clients() -> CrateResult<()> {
        // Test that the start collecting signatures method sends the correct messages to the clients
        Ok(())
    }
}
