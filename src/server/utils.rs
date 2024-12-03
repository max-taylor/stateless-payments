use anyhow::anyhow;
use tokio_tungstenite::tungstenite::{Error, Message};

use crate::errors::CrateResult;

use super::ws_message::WsMessage;

pub fn parse_ws_message(msg: Message) -> CrateResult<WsMessage> {
    if msg.is_text() {
        Ok(msg.try_into()?)
    } else if msg.is_close() {
        Err(Error::ConnectionClosed.into())
    } else {
        Err(anyhow!("Invalid message type"))
    }
}
