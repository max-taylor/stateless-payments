use std::fmt::Debug;

use futures_util::{stream::SplitSink, SinkExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    errors::CrateResult,
    types::{
        balance::BalanceProof,
        signatures::{BlsPublicKey, BlsSignature},
        transaction::{TransactionBatch, TransactionProof},
    },
};

use super::ws_message::WsMessage;

pub trait ClientTransport: Debug {
    async fn add_connection(&mut self, public_key: BlsPublicKey) -> CrateResult<()>;

    async fn send_transaction_batch(&mut self, batch: TransactionBatch) -> CrateResult<()>;

    async fn send_transaction_batch_signature(
        &mut self,
        public_key: BlsPublicKey,
        signature: BlsSignature,
    ) -> CrateResult<()>;

    async fn send_batch_to_receivers(
        &mut self,
        proof: TransactionProof,
        balance_proof: BalanceProof,
    ) -> CrateResult<()>;
}

#[derive(Debug)]
pub struct WebSocketTransport {
    ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

impl WebSocketTransport {
    pub fn new(ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) -> Self {
        Self { ws_send }
    }
}

impl ClientTransport for WebSocketTransport {
    async fn add_connection(&mut self, public_key: BlsPublicKey) -> CrateResult<()> {
        let message: Message = WsMessage::CAddConnection(public_key).into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    async fn send_transaction_batch(&mut self, batch: TransactionBatch) -> CrateResult<()> {
        let message: Message = WsMessage::CSendTransactionBatch(batch).into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    async fn send_transaction_batch_signature(
        &mut self,
        public_key: BlsPublicKey,
        signature: BlsSignature,
    ) -> CrateResult<()> {
        let message: Message =
            WsMessage::CSendTransactionBatchSignature(public_key, signature).into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    async fn send_batch_to_receivers(
        &mut self,
        proof: TransactionProof,
        balance_proof: BalanceProof,
    ) -> CrateResult<()> {
        let message: Message = WsMessage::CSendBatchToReceivers(proof, balance_proof).into();

        self.ws_send.send(message).await?;

        Ok(())
    }
}
