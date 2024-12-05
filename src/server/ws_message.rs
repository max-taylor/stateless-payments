use serde::{ser::Error, Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::types::{
    balance::BalanceProof,
    common::TransferBlock,
    signatures::{BlsPublicKey, BlsSignature},
    transaction::{TransactionBatch, TransactionProof},
};

// The WsMessage enum is used to represent the different types of messages that can be sent over the WebSocket connection.
#[derive(Debug, Serialize, Deserialize)]
pub enum WsMessage {
    // Messages prefixed with C are sent by the client
    CAddConnection(BlsPublicKey),
    CSendTransactionBatch(TransactionBatch),
    CSendTransactionBatchSignature(BlsPublicKey, BlsSignature),
    CSendBatchToReceivers(TransactionProof, BalanceProof),

    // Messages prefixed with S are sent by the server
    SSendTransactionInclusionProof(TransactionProof),
    SFinalised(TransferBlock),
    SReceiveTransaction(TransactionProof, BalanceProof),
}

impl From<WsMessage> for Message {
    fn from(ws_message: WsMessage) -> Message {
        let json = serde_json::to_string(&ws_message).unwrap();
        Message::Text(json)
    }
}

impl TryFrom<Message> for WsMessage {
    type Error = serde_json::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        match message {
            // Only support Text messages for simplicity
            Message::Text(text) => serde_json::from_str(&text),
            _ => Err(serde_json::Error::custom("Invalid message type")),
        }
    }
}
