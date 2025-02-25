use std::{sync::Arc, time::Duration};

use anyhow::anyhow;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::{error, info};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    errors::CrateResult,
    rollup::traits::RollupStateTrait,
    types::{
        balance::{BalanceProof, BalanceProofKey},
        common::{TransferBlock, U8_32},
        signatures::BlsPublicKey,
        transaction::TransactionProof,
    },
    wallet::wallet::Wallet,
    websocket::ws_message::{parse_ws_message, WsMessage},
};

use super::constants::TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS;

#[derive(Debug)]
pub struct Client {
    pub wallet: Wallet,
    ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

impl Client {
    pub async fn new(
        mut wallet: Wallet,
        rollup_state: impl RollupStateTrait + Send + Clone + Sync + 'static,
        port: u16,
    ) -> CrateResult<(
        Arc<Mutex<Self>>,
        JoinHandle<CrateResult<()>>,
        JoinHandle<CrateResult<()>>,
    )> {
        wallet.sync_rollup_state(&rollup_state).await?;

        let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", port)).await?;
        let (mut ws_send, ws_receive) = socket.split();

        // Register the wallet's public key with the server
        let message: Message = WsMessage::CAddConnection(wallet.public_key.clone()).into();
        ws_send.send(message).await?;

        let client = Arc::new(Mutex::new(Self { wallet, ws_send }));

        let automatic_sync_handler = Self::spawn_automatic_sync_thread(
            client.clone(),
            rollup_state.clone(),
            TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS,
        )
        .await?;

        let ws_receive_handler =
            Self::spawn_ws_receive_handler(client.clone(), ws_receive, rollup_state);

        Ok((client, automatic_sync_handler, ws_receive_handler))
    }

    pub async fn send_transaction_batch(&mut self) -> CrateResult<()> {
        info!("Sending transaction batch to server");

        let batch = self.wallet.produce_batch()?;
        let message: Message = WsMessage::CSendTransactionBatch(batch).into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    pub async fn validate_sign_proof_send_signature(
        &mut self,
        proof: &TransactionProof,
    ) -> CrateResult<()> {
        info!("Validating and signing proof");

        let signature = self.wallet.validate_and_sign_proof(&proof)?;

        let message: Message =
            WsMessage::CSendTransactionBatchSignature(self.wallet.public_key, signature).into();

        info!("Sending signature to server");
        self.ws_send.send(message).await?;

        Ok(())
    }

    async fn send_batch_with_root_to_receivers(&mut self, root: U8_32) -> CrateResult<()> {
        info!("Sending batch {:?} to receivers", root);

        let proof = self.wallet.balance_proof.get(&BalanceProofKey {
            root,
            public_key: self.wallet.public_key.clone().into(),
        });

        if !proof.is_some() {
            return Err(anyhow!("No proof found for the given root and public key"));
        }

        let message: Message = WsMessage::CSendBatchToReceivers(
            proof.unwrap().clone(),
            self.wallet.balance_proof.clone(),
        )
        .into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    async fn add_receiving_transaction(
        &mut self,
        proof: &TransactionProof,
        senders_balance_proof: &BalanceProof,
        rollup_state: &(impl RollupStateTrait + Send + Sync),
    ) -> CrateResult<()> {
        info!("Adding receive transaction to wallet");

        let previous_balance = self.wallet.balance;

        self.wallet
            .add_receiving_transaction(proof, senders_balance_proof, rollup_state)
            .await?;
        info!(
            "Previous balance: {}, new balance: {}",
            previous_balance, self.wallet.balance
        );

        Ok(())
    }

    // Management threads
    //
    //
    async fn spawn_automatic_sync_thread(
        client: Arc<Mutex<Client>>,
        rollup_state: impl RollupStateTrait + Send + Sync + 'static,
        sync_rate_seconds: u64,
    ) -> CrateResult<JoinHandle<CrateResult<()>>> {
        client
            .lock()
            .await
            .wallet
            .sync_rollup_state(&rollup_state)
            .await?;

        #[derive(PartialEq, Eq)]
        struct SyncState {
            deposit_total: u64,
            withdraw_total: u64,
            transfer_blocks: Vec<TransferBlock>,
        }

        let public_key = client.lock().await.wallet.public_key;

        async fn get_sync_state(
            rollup_state: &(impl RollupStateTrait + Send + Sync),
            public_key: &BlsPublicKey,
        ) -> CrateResult<SyncState> {
            Ok(SyncState {
                deposit_total: rollup_state.get_account_deposit_amount(public_key).await?,
                withdraw_total: rollup_state.get_account_withdraw_amount(public_key).await?,
                transfer_blocks: rollup_state.get_account_transfer_blocks(public_key).await?,
            })
        }

        let mut last_sync_state = get_sync_state(&rollup_state, &public_key).await?;

        Ok(tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(sync_rate_seconds)).await;

                let new_sync_state = get_sync_state(&rollup_state, &public_key).await?;

                if new_sync_state != last_sync_state {
                    if new_sync_state.transfer_blocks != last_sync_state.transfer_blocks {
                        info!(
                            "Detected new transfer blocks, extracting and sending to receivers..."
                        );
                        // Find the new transfer blocks
                        let new_transfer_blocks = new_sync_state
                            .transfer_blocks
                            .iter()
                            .filter(|block| {
                                !last_sync_state
                                    .transfer_blocks
                                    .iter()
                                    .any(|old_block| old_block == *block)
                            })
                            .cloned()
                            .collect::<Vec<TransferBlock>>();

                        for block in new_transfer_blocks {
                            client
                                .lock()
                                .await
                                .send_batch_with_root_to_receivers(block.merkle_root)
                                .await?;
                        }
                    } else {
                        info!("Detected new deposit or withdraw, syncing state...");
                        client
                            .lock()
                            .await
                            .wallet
                            .sync_rollup_state(&rollup_state)
                            .await?;
                    }
                }

                last_sync_state = new_sync_state;
            }
        }))
    }

    fn spawn_ws_receive_handler(
        client: Arc<Mutex<Client>>,
        mut ws_receive: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        rollup_state: impl RollupStateTrait + Send + Sync + 'static,
    ) -> JoinHandle<CrateResult<()>> {
        async fn handle_ws_message(
            client: Arc<Mutex<Client>>,
            msg: Result<Message, tokio_tungstenite::tungstenite::Error>,
            rollup_state: &(impl RollupStateTrait + Send + Sync),
        ) -> CrateResult<()> {
            let ws_message = parse_ws_message(msg?)?;

            match ws_message {
                WsMessage::SSendTransactionInclusionProof(proof) => {
                    client
                        .lock()
                        .await
                        .validate_sign_proof_send_signature(&proof)
                        .await?;
                }
                WsMessage::SReceiveTransaction(proof, balance_proof) => {
                    client
                        .lock()
                        .await
                        .add_receiving_transaction(&proof, &balance_proof, rollup_state)
                        .await?
                }
                _ => {
                    return Err(anyhow!("Invalid message type"));
                }
            }

            Ok(())
        }

        tokio::spawn(async move {
            loop {
                if let Some(msg) = ws_receive.next().await {
                    if let Err(e) = handle_ws_message(client.clone(), msg, &rollup_state).await {
                        error!("Error handling message: {:?}", e);
                    }
                }
            }
        })
    }

    pub async fn shutdown(&mut self) -> CrateResult<()> {
        let _ = timeout(Duration::from_secs(2), self.ws_send.close()).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::rollup::mock_rollup_memory::MockRollupMemory;
    use crate::rollup::traits::MockRollupStateTrait;
    use crate::websocket::client::constants::TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS;
    use crate::websocket::server::server_state::ServerState;

    use super::*;

    async fn setup() -> CrateResult<(
        Arc<Mutex<ServerState>>,
        Arc<Mutex<Client>>,
        Arc<Mutex<MockRollupMemory>>,
    )> {
        let rollup_state = Arc::new(Mutex::new(MockRollupMemory::new()));
        let (server, _, port) = ServerState::new_with_ws_server(rollup_state.clone(), None).await?;
        // Delay 1s to allow the server to start
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let (client, _, _) = Client::new(Wallet::new(None), rollup_state.clone(), port).await?;

        Ok((server.clone(), client, rollup_state))
    }

    const SLEEP_TIME_SECONDS: u64 = TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS + 1;

    #[tokio::test]
    async fn test_client_auto_syncs_deposits() -> CrateResult<()> {
        let (_, client, mut rollup_state) = setup().await?;

        let client_public_key = client.lock().await.wallet.public_key.clone();

        rollup_state.add_deposit(&client_public_key, 100).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(SLEEP_TIME_SECONDS)).await;

        let client_balance = client.lock().await.wallet.balance;

        assert_eq!(client_balance, 100);

        Ok(())
    }

    #[tokio::test]
    async fn test_client_auto_syncs_withdraws() -> CrateResult<()> {
        let (_, client, mut rollup_state) = setup().await?;

        let client_public_key = client.lock().await.wallet.public_key.clone();

        rollup_state.add_deposit(&client_public_key, 100).await?;
        rollup_state.add_withdraw(&client_public_key, 50).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(SLEEP_TIME_SECONDS)).await;

        let client_balance = client.lock().await.wallet.balance;

        assert_eq!(client_balance, 50);

        Ok(())
    }
}
