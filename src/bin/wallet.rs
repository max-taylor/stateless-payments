use std::env;

use cli::user_input::spawn_user_input_handler;
use stateless_bitcoin_l2::{
    constants::WEBSOCKET_PORT, errors::CrateResult, rollup::mock_rollup_fs::MockRollupFS,
    wallet::wallet::Wallet, websocket::client::client::Client,
};

mod cli;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    // Collect command-line arguments into a vector
    let args: Vec<String> = env::args().collect();

    let wallet_name = if args.len() > 1 {
        Some(args[1].clone())
    } else {
        None
    };

    let rollup_state = MockRollupFS::new()?;

    let (client, automatic_sync_handler, ws_receiver_handler) = Client::new(
        Wallet::new(wallet_name),
        rollup_state.clone(),
        WEBSOCKET_PORT,
    )
    .await?;

    {
        let public_key = client.lock().await.wallet.public_key.clone();

        println!("Welcome to the L2 wallet CLI");

        println!(
            "Your public key is: {}",
            serde_json::to_string(&public_key)?,
        );
    }

    let (user_input_result, ws_handler_result, automatic_sync_handler_result) = tokio::try_join!(
        spawn_user_input_handler(client.clone(), rollup_state),
        ws_receiver_handler,
        automatic_sync_handler
    )?;

    if let Err(e) = user_input_result {
        eprintln!("User input error: {}", e);
    }

    if let Err(e) = ws_handler_result {
        eprintln!("WS handler error: {}", e);
    }

    if let Err(e) = automatic_sync_handler_result {
        eprintln!("Automatic sync handler error: {}", e);
    }

    // TODO: Need to handle CTRL+C signal to gracefully shutdown the client and close connection
    // client.shutdown().await?;

    Ok(())
}
