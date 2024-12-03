use futures_util::SinkExt;
use stateless_bitcoin_l2::{
    client::client::Client, constants::WEBSOCKET_PORT, errors::CrateResult,
    server::ws_message::WsMessage,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let client = Client::new();

    let (mut socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let message: Message = WsMessage::CAddConnection(client.public_key.clone()).into();

    // dbg!(&message);

    socket.send(message).await?;

    socket.close(None).await?;

    // match run_aggregator_server().await {
    //     Ok(_) => {}
    //     Err(e) => eprintln!("Server exited with error: {}", e),
    // }

    Ok(())
}
