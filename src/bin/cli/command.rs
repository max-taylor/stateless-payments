enum Command {
    SendTransaction,
}

impl From<&str> for Command {
    fn from(s: &str) -> Self {
        match s {
            "send" => Command::SendTransaction,
            _ => panic!("Invalid command"),
        }
    }
}
