use std::sync::Arc;

use cli::user_input::spawn_user_input_handler;
use stateless_bitcoin_l2::{
    client::client::Client,
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    rollup::{mock_rollup_fs::MockRollupFS, traits::MockRollupStateTrait},
    wallet::wallet::Wallet,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tokio_tungstenite::connect_async;

mod cli;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let mut rollup_state = MockRollupFS::new()?;
    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let client = Arc::new(Mutex::new(Client::new(Wallet::new(), socket).await?));

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
        spawn_ws_handler(client.clone())
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

fn spawn_ws_handler(client: Arc<Mutex<Client>>) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
        loop {
            let mut client = client.lock().await;
            client.send_transaction_batch().await?;
        }
    })
}
