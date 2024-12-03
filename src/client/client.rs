use anyhow::anyhow;
use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    errors::CrateResult, server::ws_message::WsMessage, types::common::BlsPublicKey,
    wallet::wallet::Wallet,
};

pub struct Client {
    wallet: Wallet,
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Client {
    pub async fn new(
        wallet: Wallet,
        mut socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> CrateResult<Self> {
        // Register the wallet's public key with the server
        let message: Message = WsMessage::CAddConnection(wallet.public_key.clone()).into();
        socket.send(message).await?;

        Ok(Self { wallet, socket })
    }

    pub async fn send_transaction_batch(&mut self) -> CrateResult<()> {
        if self.wallet.transaction_batch.transactions.is_empty() {
            return Err(anyhow!("Transaction batch is empty"));
        }

        let message: Message =
            WsMessage::CSendTransactionBatch(self.wallet.transaction_batch.clone()).into();

        self.socket.send(message).await?;

        Ok(())
    }

    pub fn append_transaction_to_batch(
        &mut self,
        to: BlsPublicKey,
        amount: u64,
    ) -> CrateResult<()> {
        self.wallet.append_transaction_to_batch(to, amount)?;

        Ok(())
    }

    pub async fn shutdown(&mut self) -> CrateResult<()> {
        self.socket.close(None).await?;

        Ok(())
    }
}
