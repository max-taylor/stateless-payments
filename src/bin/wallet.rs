use log::info;
use stateless_bitcoin_l2::{
    client::client::Client, constants::WEBSOCKET_PORT, errors::CrateResult, wallet::wallet::Wallet,
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() -> CrateResult<()> {
    env_logger::init();

    let (socket, _) = connect_async(format!("ws://127.0.0.1:{}", WEBSOCKET_PORT)).await?;

    let mut client = Client::new(Wallet::new(), socket).await?;

    // client
    //     .wallet
    //     .append_transaction_to_batch(client.wallet.public_key.clone(), 100)?;

    // Start tasks for user input and WebSocket handling
    tokio::select! {
        _ = handle_user_input() => {},
        // _ = handle_websocket_messages(read) => {},
    }

    // Needs to run if the user exits the CLI with CTRL+C
    client.shutdown().await?;

    Ok(())
}

async fn handle_user_input() -> CrateResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin).lines();
    let mut stdout = io::BufWriter::new(stdout);

    stdout.write_all(b"> ").await?;
    stdout.flush().await?;

    while let Ok(Some(line)) = reader.next_line().await {
        let command = line.trim();

        if command == "exit" {
            info!("Exiting CLI");
            break;
        }

        let command_string = format!("[Command]: {}\n", command);

        stdout.write_all(command_string.as_bytes()).await?;
        stdout.write_all(b"> ").await?;
        stdout.flush().await?;

        // // Send the command over WebSocket
        // if let Err(e) = tx.send(command.to_string()).await {
        //     error!("Failed to send command: {:?}", e);
        // }
    }

    Ok(())
}
