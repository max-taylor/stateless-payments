use bitcoincore_rpc::bitcoin::key::rand;

pub type Salt = [u8; 32];

pub fn generate_salt() -> Salt {
    rand::random()
}
