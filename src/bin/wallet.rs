use std::sync::Arc;

use blsful::inner_types::GroupEncoding;
use blsful::BlsSignatureImpl;
use log::info;
use serde::Serialize;
use stateless_bitcoin_l2::{
    client::client::Client, constants::WEBSOCKET_PORT, errors::CrateResult,
    types::common::BlsPublicKey, wallet::wallet::Wallet,
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio_tungstenite::connect_async;

use crate::cli::command::Command;

mod cli;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    // let string_test = "[71, 50, 65, 102, 102, 105, 110, 101, 40, 120, 61, 70, 113, 50, 40, 70, 112, 40, 48, 120, 48, 97, 57, 50, 52, 56, 53, 98, 57, 57, 48, 99, 102, 48, 48, 100, 51, 48, 100, 48, 98, 52, 54, 52, 56, 50, 100, 54, 101, 48, 97, 48, 50, 97, 102, 98, 97, 98, 99, 49, 57, 54, 101, 55, 50, 51, 57, 51, 100, 57, 101, 53, 102, 101, 97, 51, 53, 99, 53, 51, 54, 57, 49, 100, 51, 101, 51, 99, 102, 55, 53, 97, 51, 100, 100, 99, 100, 97, 53, 53, 48, 50, 49, 50, 102, 101, 55, 97, 49, 55, 55, 53, 102, 99, 48, 55, 41, 32, 43, 32, 70, 112, 40, 48, 120, 48, 102, 55, 97, 52, 100, 49, 52, 97, 52, 101, 98, 57, 56, 57, 48, 97, 49, 99, 57, 53, 57, 48, 55, 56, 99, 98, 102, 52, 100, 54, 56, 52, 102, 49, 50, 99, 98, 50, 55, 53, 55, 98, 49, 98, 53, 97, 98, 54, 101, 102, 50, 102, 49, 53, 102, 99, 49, 102, 51, 57, 52, 50, 99, 51, 54, 49, 57, 50, 57, 102, 55, 100, 99, 99, 100, 97, 97, 56, 48, 102, 55, 48, 102, 55, 48, 101, 57, 53, 48, 49, 48, 55, 97, 53, 97, 41, 32, 42, 32, 117, 41, 44, 32, 121, 61, 70, 113, 50, 40, 70, 112, 40, 48, 120, 49, 54, 57, 97, 55, 102, 49, 101, 56, 100, 53, 55, 48, 51, 49, 98, 102, 50, 55, 102, 55, 100, 56, 49, 53, 53, 49, 101, 54, 56, 99, 51, 56, 100, 98, 57, 99, 99, 98, 100, 100, 49, 101, 57, 49, 51, 49, 56, 100, 57, 52, 55, 102, 57, 53, 102, 99, 97, 49, 52, 54, 98, 52, 50, 56, 52, 50, 52, 101, 51, 102, 50, 55, 53, 52, 56, 49, 54, 97, 50, 101, 102, 50, 53, 99, 102, 100, 50, 97, 99, 51, 53, 97, 99, 56, 54, 41, 32, 43, 32, 70, 112, 40, 48, 120, 48, 56, 99, 55, 102, 97, 56, 49, 49, 102, 102, 102, 53, 49, 51, 54, 99, 97, 49, 57, 100, 56, 98, 55, 52, 98, 52, 98, 52, 50, 99, 50, 56, 49, 54, 55, 100, 101, 52, 51, 51, 55, 56, 56, 49, 102, 99, 55, 51, 49, 54, 99, 49, 55, 101, 52, 50, 49, 56, 48, 54, 54, 54, 49, 102, 56, 100, 55, 99, 49, 98, 55, 100, 48, 48, 55, 97, 50, 49, 53, 99, 51, 97, 51, 102, 50, 98, 57, 51, 98, 100, 48, 97, 101, 51, 52, 41, 32, 42, 32, 117, 41, 41]";
    // //
    // let test_key = BlsPublicKey::default();
    // dbg!(test_key.to_string());
    // let key = BlsPublicKey::try_from(string_test.as_bytes()).unwrap();
    // dbg!(key.to_string());

    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let client = Client::new(Wallet::new(), socket).await?;

    let key = serde_json::to_string(&client.wallet.public_key)?;

    let key: BlsPublicKey = serde_json::from_str(&key)?;

    dbg!(key);

    println!(
        "Your PubKey: {:?}",
        serde_json::to_string(&client.wallet.public_key)?
    );

    // Start tasks for user input and signal handling
    tokio::select! {
        _ = handle_user_input(client) => {},
    }

    // client.shutdown().await?;

    Ok(())
}

async fn handle_user_input(mut client: Client) -> CrateResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin).lines();
    let mut stdout = io::BufWriter::new(stdout);

    stdout.write_all(b"> ").await?;
    stdout.flush().await?;

    while let Ok(Some(line)) = reader.next_line().await {
        let command: CrateResult<Command> = line.trim().try_into();

        if let Err(e) = command {
            stdout
                .write_all(format!("Invalid command: {}\n", e).as_bytes())
                .await?;
        } else {
            let command = command.unwrap();

            match command {
                Command::AppendTransactionToBatch(to, amount) => {
                    info!(
                        "Received send_transaction command, to: {:?}, amount: {}",
                        to, amount
                    );
                    client.wallet.append_transaction_to_batch(to, amount)?;
                }
                Command::SendBatchToServer => {
                    info!("Received send_batch command");
                }
                Command::Exit => {
                    info!("Exiting CLI");
                    break;
                }
            }
        }

        stdout.write_all(b"> ").await?;
        stdout.flush().await?;
    }

    Ok(())
}
