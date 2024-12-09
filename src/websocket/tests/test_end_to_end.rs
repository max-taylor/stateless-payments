#![allow(unused_imports)] // Weirdly needed for some reason
use std::sync::Arc;

use log::info;
use tokio::sync::Mutex;

use crate::{
    errors::CrateResult,
    rollup::{mock_rollup_memory::MockRollupMemory, traits::MockRollupStateTrait},
    wallet::wallet::Wallet,
    websocket::{
        client::{client::Client, constants::TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS},
        server::{server::spawn_block_producer, server_state::ServerState},
    },
};

const SYNC_SLEEP_TIME: u64 = TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS + 1;

// TODO: Move to end to end tests
#[tokio::test]
async fn test_client_auto_syncs_transfers_and_contacts_receiver() -> CrateResult<()> {
    env_logger::init();
    info!("Starting");
    let mut rollup_state = Arc::new(Mutex::new(MockRollupMemory::new()));
    let (server, _, port) = ServerState::new_with_ws_server(rollup_state.clone(), None).await?;
    let _ = spawn_block_producer(server.clone(), Some(1));

    // Delay 1s to allow the server to start
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let (client, _, _) = Client::new(Wallet::new(None), rollup_state.clone(), port).await?;
    let (receiver, _, _) = Client::new(Wallet::new(None), rollup_state.clone(), port).await?;

    let client_public_key = client.lock().await.wallet.public_key.clone();

    rollup_state.add_deposit(&client_public_key, 100).await?;

    // Allows the client to sync with the rollup state
    tokio::time::sleep(tokio::time::Duration::from_secs(SYNC_SLEEP_TIME)).await;

    client
        .lock()
        .await
        .wallet
        .append_transaction_to_batch(receiver.lock().await.wallet.public_key.clone(), 50)?;

    // Sends the batch to the server
    client.lock().await.send_transaction_batch().await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;

    assert_eq!(receiver.lock().await.wallet.balance, 50);
    assert_eq!(client.lock().await.wallet.balance, 50);

    Ok(())
}
