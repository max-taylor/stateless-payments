use std::sync::Arc;

use anyhow::anyhow;
use cli::user_input::spawn_user_input_handler;
use futures_util::{stream::SplitStream, StreamExt};
use stateless_bitcoin_l2::{
    client::client::Client,
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    rollup::{mock_rollup_fs::MockRollupFS, traits::MockRollupStateTrait},
    server::{utils::parse_ws_message, ws_message::WsMessage},
    wallet::wallet::Wallet,
};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

mod cli;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let mut rollup_state = MockRollupFS::new()?;
    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;
    let (ws_send, ws_receive) = socket.split();

    let client = Arc::new(Mutex::new(Client::new(Wallet::new(), ws_send).await?));

    {
        let mut client = client.lock().await;
        rollup_state.add_deposit(client.wallet.public_key.clone(), 100)?;

        client.wallet.sync_rollup_state(&rollup_state)?;

        println!("Welcome to the L2 wallet CLI");

        println!(
            "Your public key is: {}",
            serde_json::to_string(&client.wallet.public_key)?,
        );
    }

    let (user_input_result, ws_handler_result) = tokio::try_join!(
        spawn_user_input_handler(client.clone()),
        spawn_ws_receive_handler(client.clone(), ws_receive)
    )?;

    if let Err(e) = user_input_result {
        eprintln!("User input error: {}", e);
    }

    if let Err(e) = ws_handler_result {
        eprintln!("WS handler error: {}", e);
    }

    // TODO: Need to handle CTRL+C signal to gracefully shutdown the client and close connection
    // client.shutdown().await?;

    Ok(())
}

fn spawn_ws_receive_handler(
    client: Arc<Mutex<Client>>,
    mut ws_receive: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        loop {
            if let Some(msg) = ws_receive.next().await {
                let ws_message = parse_ws_message(msg?)?;

                match ws_message {
                    WsMessage::SStartCollectingSignatures => {
                        let mut client = client.lock().await;
                        client.send_transaction_batch().await?;
                    }
                    _ => {
                        return Err(anyhow!("Invalid message type"));
                    }
                }
            }
        }
    })
}
