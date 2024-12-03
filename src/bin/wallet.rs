use futures_util::SinkExt;
use stateless_bitcoin_l2::{
    constants::WEBSOCKET_PORT, errors::CrateResult, server::ws_message::WsMessage,
    wallet::wallet::Wallet,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let client = Wallet::new();

    let (mut socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let message: Message = WsMessage::CAddConnection(client.public_key.clone()).into();

    socket.send(message).await?;

    socket.close(None).await?;

    Ok(())
}
