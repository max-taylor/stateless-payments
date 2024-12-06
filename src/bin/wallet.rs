use std::sync::Arc;

use anyhow::anyhow;
use cli::user_input::spawn_user_input_handler;
use futures_util::{stream::SplitStream, StreamExt};
use log::error;
use stateless_bitcoin_l2::{
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    rollup::mock_rollup_fs::MockRollupFS,
    wallet::wallet::Wallet,
    websocket::{
        client::client::Client,
        ws_message::{parse_ws_message, WsMessage},
    },
};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

mod cli;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let rollup_state = MockRollupFS::new()?;
    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;
    let (ws_send, ws_receive) = socket.split();

    let client = Arc::new(Mutex::new(
        Client::new(Wallet::new(None), rollup_state, ws_send).await?,
    ));

    {
        let mut client = client.lock().await;
        client.add_mock_deposit(100).await?;

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
                if let Err(e) = handle_ws_message(client.clone(), msg).await {
                    error!("Error handling message: {:?}", e);
                }
            }
        }
    })
}

async fn handle_ws_message(
    client: Arc<Mutex<Client>>,
    msg: Result<Message, tokio_tungstenite::tungstenite::Error>,
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
                .add_receiving_transaction(&proof, &balance_proof)
                .await?
        }
        _ => {
            return Err(anyhow!("Invalid message type"));
        }
    }

    Ok(())
}
