use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{errors::CrateResult, server::ws_message::WsMessage, wallet::wallet::Wallet};

pub struct Client {
    pub wallet: Wallet,
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
        let batch = self.wallet.produce_batch()?;
        let message: Message = WsMessage::CSendTransactionBatch(batch).into();

        self.socket.send(message).await?;

        Ok(())
    }

    pub async fn shutdown(&mut self) -> CrateResult<()> {
        self.socket.close(None).await?;

        Ok(())
    }
}
