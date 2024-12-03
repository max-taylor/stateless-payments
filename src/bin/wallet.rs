use stateless_bitcoin_l2::{
    client::client::Client, constants::WEBSOCKET_PORT, errors::CrateResult, wallet::wallet::Wallet,
};
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let mut client = Client::new(Wallet::new(), socket).await?;

    client.shutdown().await?;

    Ok(())
}
