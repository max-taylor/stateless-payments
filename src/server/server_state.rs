use std::collections::HashMap;

use crate::types::public_key::BlsPublicKeyWrapper;

pub struct Connection {}

pub struct ServerState {
    connections: HashMap<BlsPublicKeyWrapper, Connection>,
}

impl ServerState {
    pub fn new() -> ServerState {
        ServerState {
            connections: HashMap::new(),
        }
    }

    pub fn add_connection(&mut self, public_key: BlsPublicKeyWrapper, connection: Connection) {
        self.connections.insert(public_key, connection);
    }

    pub fn remove_connection(&mut self, public_key: &BlsPublicKeyWrapper) {
        self.connections.remove(public_key);
    }
}
