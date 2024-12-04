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
            match handle_new_line(client.clone(), &line).await {
                Ok(Command::Exit) => {
                    info!("Exiting CLI");
                    break;
                }
                Err(e) => {
                    stdout
                        .write_all(format!("Error: {}\n", e).as_bytes())
                        .await?;
                }
                _ => {}
            }

            stdout.write_all(b"> ").await?;
            stdout.flush().await?;
        }

        Err(anyhow!("User input handler exited"))
    })
}

async fn handle_new_line(client: Arc<Mutex<Client>>, line: &str) -> CrateResult<Command> {
    let command: Command = line.trim().try_into()?;

    match command {
        Command::AppendTransactionToBatch(to, amount) => {
            client
                .lock()
                .await
                .wallet
                .append_transaction_to_batch(to, amount)?;

            ()
        }
        Command::SendBatchToServer => client.lock().await.send_transaction_batch().await?,
        Command::PrintBalance => {
            println!("Balance: {}", client.lock().await.wallet.balance);
        }
        _ => {
            return Err(anyhow!("Invalid command"));
        }
    }

    Ok(command)
}
