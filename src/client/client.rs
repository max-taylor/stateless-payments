use futures_util::{stream::SplitSink, SinkExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{errors::CrateResult, server::ws_message::WsMessage, wallet::wallet::Wallet};

#[derive(Debug)]
pub struct Client {
    pub wallet: Wallet,
    ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

impl Client {
    pub async fn new(
        wallet: Wallet,
        mut ws_send: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    ) -> CrateResult<Self> {
        // Register the wallet's public key with the server
        let message: Message = WsMessage::CAddConnection(wallet.public_key.clone()).into();
        ws_send.send(message).await?;

        Ok(Self { wallet, ws_send })
    }

    pub async fn send_transaction_batch(&mut self) -> CrateResult<()> {
        let batch = self.wallet.produce_batch()?;
        let message: Message = WsMessage::CSendTransactionBatch(batch).into();

        self.ws_send.send(message).await?;

        Ok(())
    }

    // pub async fn shutdown(&mut self) -> CrateResult<()> {
    //     self.ws_send.close(None).await?;
    //
    //     Ok(())
    // }
}
