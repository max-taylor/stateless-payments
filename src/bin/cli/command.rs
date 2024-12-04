use anyhow::anyhow;
use stateless_bitcoin_l2::types::common::BlsPublicKey;

#[derive(Debug, PartialEq)]
pub enum Command {
    AppendTransactionToBatch(BlsPublicKey, u64),
    SendBatchToServer,
    Exit,
}

impl TryFrom<&str> for Command {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let parts = value.split_whitespace().collect::<Vec<&str>>();

        if parts.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        match parts[0] {
            "append_tx" => {
                if parts.len() != 3 {
                    return Err(anyhow!(format!(
                        "Invalid number of arguments for send_transaction, expected 3 got {}",
                        parts.len()
                    )));
                }

                // Not sure why we need to do this, but validation fails otherwise
                let formatted_string = format!("\"{}\"", parts[1]);

                let public_key: BlsPublicKey = serde_json::from_str(&formatted_string).unwrap();

                let amount = parts[2].parse::<u64>()?;

                Ok(Command::AppendTransactionToBatch(public_key, amount))
            }
            "send_batch" => Ok(Command::SendBatchToServer),
            "exit" => Ok(Command::Exit),
            _ => Err(anyhow!("Invalid command")),
        }
    }
}

#[cfg(test)]
mod tests {
    use stateless_bitcoin_l2::errors::CrateResult;

    use super::*;

    #[test]
    fn test_append_tx_to_batch() -> CrateResult<()> {
        let pubkey_string = "808868b2d0b654328c66f5b005758db14415ed1e2a6db7eb9177721cd4d55a332b0b2805b531c4b71308af26827526ed19ba9745dccfba815b7411ef93f26111e7ed041466aa724f5ce1c4b074cf957ea874ac72b5ae29878cbbfed10095f45d";
        let command_string = format!("append_tx {} 100", pubkey_string);
        let command = Command::try_from(command_string.as_str())?;

        match command {
            Command::AppendTransactionToBatch(_, amount) => {
                assert_eq!(amount, 100);
            }
            _ => assert!(false, "Append transaction to batch not parsed correctly"),
        }

        Ok(())
    }
}
