use std::{fs::OpenOptions, sync::Arc};

use anyhow::anyhow;
use cli::user_input::spawn_user_input_handler;
use fs2::FileExt;
use futures_util::{stream::SplitStream, StreamExt};
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::from_reader;
use stateless_bitcoin_l2::{
    client::client::Client,
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    rollup::mock_rollup_fs::MockRollupFS,
    server::{utils::parse_ws_message, ws_message::WsMessage},
    types::{
        common::{BlsSignature, TransferBlockSignature, U8_32},
        public_key::AccountTotals,
    },
    wallet::wallet::Wallet,
};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

mod cli;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransferBlock {
    pub signature: TransferBlockSignature,
    pub merkle_root: U8_32,
}

#[derive(Debug, Serialize, Deserialize)]
struct RollupState {
    withdraw_totals: AccountTotals,
    deposit_totals: AccountTotals,
    transfer_blocks: Vec<TransferBlock>,
}

fn read_state_from_fs() -> CrateResult<RollupState> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("rollup_state.json")?;

    file.lock_exclusive()?;

    let state = from_reader(&file)?;

    file.unlock().expect("Unable to unlock file");

    Ok(state)
}

#[tokio::main]
async fn main() -> CrateResult<()> {
    println!("Hello, world!");
    let state = read_state_from_fs()?;
    dbg!(state);
    return Ok(());

    env_logger::init();

    let signature_str = r#"{"MessageAugmentation":"80343f5c35322c2ee2d615ce8534b69c68d4892167845b0cc361b99ccdf5a3154eed0ae1f19ccd2dbc02f963602e93dc"},"99018bab54f5cf8f4928f5a8514c45cc1311e02bff4893c31fac568640df5c0f35a073b72a89b98b80b6967ba3f848c713e4e2dc11b0dd9c7e5276ae9b08de2378b70f729380ed2ad8869d7f85292628160f1d56f693a887d346998df1614688"]"#;
    let signature: BlsSignature = serde_json::from_str(&signature_str)?;

    let rollup_state = MockRollupFS::new()?;
    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;
    let (ws_send, ws_receive) = socket.split();

    let client = Arc::new(Mutex::new(
        Client::new(Wallet::new(), rollup_state, ws_send).await?,
    ));

    {
        let mut client = client.lock().await;
        client.add_mock_deposit(100)?;

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
        WsMessage::SReceiveTransaction(proof, balance_proof) => client
            .lock()
            .await
            .add_receiving_transaction(&proof, &balance_proof)?,
        _ => {
            return Err(anyhow!("Invalid message type"));
        }
    }

    Ok(())
}
