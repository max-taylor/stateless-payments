use bitcoincore_rpc::bitcoin::key::rand;

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    rand::random()
}
