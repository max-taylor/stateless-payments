use std::sync::Arc;

use anyhow::anyhow;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::{error, info};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    errors::CrateResult,
    rollup::traits::RollupStateTrait,
    types::{
        balance::{BalanceProof, BalanceProofKey},
        common::U8_32,
        signatures::BlsPublicKey,
        transaction::TransactionProof,
    },
    wallet::wallet::Wallet,
    websocket::ws_message::{parse_ws_message, WsMessage},
};

#[derive(Debug)]
pub struct Client {
    pub wallet: Wallet,
    ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

impl Client {
    pub async fn new(
        wallet: Wallet,
        rollup_state: impl RollupStateTrait + Send + Clone + Sync + 'static,
        socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> CrateResult<(
        Arc<Mutex<Self>>,
        JoinHandle<CrateResult<()>>,
        JoinHandle<CrateResult<()>>,
    )> {
        let (mut ws_send, ws_receive) = socket.split();

        // Register the wallet's public key with the server
        let message: Message = WsMessage::CAddConnection(wallet.public_key.clone()).into();
        ws_send.send(message).await?;

        let client = Arc::new(Mutex::new(Self { wallet, ws_send }));

        let automatic_sync_handler =
            Self::spawn_automatic_sync_thread(client.clone(), rollup_state.clone(), 10).await?;

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

        info!("Validated proof, sending signature to server");
        self.ws_send.send(message).await?;

        Ok(())
    }

    pub async fn finalise_batch(&mut self, root: U8_32) -> CrateResult<()> {
        info!("Finalising batch with root: {:?}", root);

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
            total_transfer_blocks: u64,
        }

        let public_key = client.lock().await.wallet.public_key;

        async fn get_sync_state(
            rollup_state: &(impl RollupStateTrait + Send + Sync),
            public_key: &BlsPublicKey,
        ) -> CrateResult<SyncState> {
            Ok(SyncState {
                deposit_total: rollup_state.get_account_deposit_amount(public_key).await?,
                withdraw_total: rollup_state.get_account_withdraw_amount(public_key).await?,
                total_transfer_blocks: rollup_state
                    .get_account_transfer_blocks(public_key)
                    .await?
                    .len()
                    .try_into()?,
            })
        }

        let mut last_sync_state = get_sync_state(&rollup_state, &public_key).await?;

        Ok(tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(sync_rate_seconds)).await;

                let new_sync_state = get_sync_state(&rollup_state, &public_key).await?;

                // TODO: Update to message all receivers when total transfer blocks change
                if new_sync_state != last_sync_state {
                    client
                        .lock()
                        .await
                        .wallet
                        .sync_rollup_state(&rollup_state)
                        .await?;
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
                WsMessage::SFinalised(block) => {
                    client
                        .lock()
                        .await
                        .finalise_batch(block.merkle_root)
                        .await?
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

    // pub async fn shutdown(&mut self) -> CrateResult<()> {
    //     self.ws_send.close(None).await?;
    //
    //     Ok(())
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::protocol::WebSocket;

    // async fn create_mock_websocket_stream() -> WebSocket<MaybeTlsStream<TcpStream>> {
    //     let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    //     let addr = listener.local_addr().unwrap();
    //
    //     tokio::spawn(async move {
    //         let (stream, _) = listener.accept().await.unwrap();
    //         let _ = accept_async(stream).await.unwrap();
    //     });
    //
    //     let stream = TcpStream::connect(addr).await.unwrap();
    //     let (ws_stream, _) = tokio_tungstenite::client_async("ws://localhost", stream)
    //         .await
    //         .unwrap();
    //
    //     ws_stream
    // }

    // #[tokio::test]
    // async fn test_client_new() {
    //     let wallet = Wallet::new(); // Assuming you have a Wallet::new() method
    //                                 // let rollup_state = MockRollupState::new(); // Assuming you have a MockRollupState
    //
    //     // let socket = create_mock_websocket_stream().await;
    //
    //     let result = Client::new(wallet, rollup_state, socket).await;
    //
    //     assert!(result.is_ok());
    // }

    // Add more tests as needed
}
