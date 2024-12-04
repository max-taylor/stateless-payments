use std::sync::Arc;

use anyhow::anyhow;
use log::info;
use stateless_bitcoin_l2::{client::client::Client, errors::CrateResult};
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt},
    sync::Mutex,
    task::JoinHandle,
};

use crate::cli::command::Command;

// This function handles user input and sends it to the server
//
// It is very intentional about which errors are propogated and which aren't
// This is because we want to keep the CLI running when we encounter non-fatal errors, such as bad
// inputs
pub fn spawn_user_input_handler(client: Arc<Mutex<Client>>) -> JoinHandle<CrateResult<()>> {
    tokio::spawn(async move {
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
                        match client
                            .lock()
                            .await
                            .wallet
                            .append_transaction_to_batch(to, amount)
                        {
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
                    Command::SendBatchToServer => {
                        match client.lock().await.send_transaction_batch().await {
                            Ok(_) => {
                                println!("Batch sent to server");
                            }
                            Err(e) => {
                                stdout
                                    .write_all(format!("Error: {}\n", e).as_bytes())
                                    .await?;
                            }
                        }
                    }
                    Command::Exit => {
                        info!("Exiting CLI");
                        break;
                    }
                    Command::PrintBalance => {
                        stdout
                            .write_all(
                                format!("Balance: {}\n", client.lock().await.wallet.balance)
                                    .as_bytes(),
                            )
                            .await?;
                    }
                }
            }

            stdout.write_all(b"> ").await?;
            stdout.flush().await?;
        }

        Err(anyhow!("User input handler exited"))
    })
}
