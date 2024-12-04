use log::info;
use stateless_bitcoin_l2::{
    client::client::Client,
    constants::WEBSOCKET_PORT,
    errors::CrateResult,
    rollup::{mock_rollup_fs::MockRollupFS, traits::MockRollupStateTrait},
    wallet::wallet::Wallet,
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio_tungstenite::connect_async;

use crate::cli::command::Command;

mod cli;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let mut rollup_state = MockRollupFS::new()?;
    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let mut client = Client::new(Wallet::new(), socket).await?;

    rollup_state.add_deposit(client.wallet.public_key.clone(), 100)?;

    client.wallet.sync_rollup_state(&rollup_state)?;

    println!("Welcome to the L2 wallet CLI");
    println!(
        "Your public key is: {}",
        serde_json::to_string(&client.wallet.public_key)?,
    );

    // Start tasks for user input and signal handling
    let result = tokio::select! {
        result = handle_user_input(client) => result,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    // client.shutdown().await?;

    Ok(())
}

// This function handles user input and sends it to the server
//
// It is very intentional about which errors are propogated and which aren't
// This is because we want to keep the CLI running when we encounter non-fatal errors, such as bad
// inputs
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
            match command.unwrap() {
                Command::AppendTransactionToBatch(to, amount) => {
                    match client.wallet.append_transaction_to_batch(to, amount) {
                        Ok(_) => {
                            stdout.write_all(b"Transaction appended to batch\n").await?;
                        }
                        Err(e) => {
                            stdout
                                .write_all(format!("Error: {}\n", e).as_bytes())
                                .await?;
                        }
                    }
                }
                Command::SendBatchToServer => match client.send_transaction_batch().await {
                    Ok(_) => {
                        println!("Batch sent to server");
                    }
                    Err(e) => {
                        stdout
                            .write_all(format!("Error: {}\n", e).as_bytes())
                            .await?;
                    }
                },
                Command::Exit => {
                    info!("Exiting CLI");
                    break;
                }
                Command::PrintBalance => {
                    stdout
                        .write_all(format!("Balance: {}\n", client.wallet.balance).as_bytes())
                        .await?;
                }
            }
        }

        stdout.write_all(b"> ").await?;
        stdout.flush().await?;
    }

    Ok(())
}
