use anyhow::anyhow;
use futures_util::{stream::SplitSink, SinkExt};
use log::info;
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    errors::CrateResult,
    rollup::{mock_rollup_fs::MockRollupFS, traits::MockRollupStateTrait},
    types::{
        balance::{BalanceProof, BalanceProofKey},
        common::U8_32,
        transaction::TransactionProof,
    },
    wallet::wallet::Wallet,
    websocket::ws_message::WsMessage,
};

#[derive(Debug)]
pub struct Client {
    pub wallet: Wallet,
    pub rollup_state: MockRollupFS,
    ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

impl Client {
    pub async fn new(
        wallet: Wallet,
        rollup_state: MockRollupFS,
        mut ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    ) -> CrateResult<Self> {
        // Register the wallet's public key with the server
        let message: Message = WsMessage::CAddConnection(wallet.public_key.clone()).into();
        ws_send.send(message).await?;

        Ok(Self {
            wallet,
            rollup_state,
            ws_send,
        })
    }

    pub async fn add_mock_deposit(&mut self, amount: u64) -> CrateResult<()> {
        self.rollup_state
            .add_deposit(self.wallet.public_key.clone(), amount)
            .await?;

        self.wallet.sync_rollup_state(&self.rollup_state).await?;

        Ok(())
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

        info!("Sending transaction to receiver");
        dbg!(self.wallet.balance_proof.clone());
        let message: Message = WsMessage::CSendBatchToReceivers(
            proof.unwrap().clone(),
            self.wallet.balance_proof.clone(),
        )
        .into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    pub async fn add_receiving_transaction(
        &mut self,
        proof: &TransactionProof,
        senders_balance_proof: &BalanceProof,
    ) -> CrateResult<()> {
        info!("Adding receive transaction to wallet");

        let previous_balance = self.wallet.balance;

        self.wallet
            .add_receiving_transaction(proof, senders_balance_proof, &self.rollup_state)
            .await?;
        info!(
            "Previous balance: {}, new balance: {}",
            previous_balance, self.wallet.balance
        );

        Ok(())
    }

    // pub async fn shutdown(&mut self) -> CrateResult<()> {
    //     self.ws_send.close(None).await?;
    //
    //     Ok(())
    // }
}
