use blsful::inner_types::G1Projective;
use blsful::{AggregateSignature, Bls12381G1Impl, PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};

type BlsType = Bls12381G1Impl;
pub type BlsPublicKey = PublicKey<BlsType>;
pub type BlsSignature = Signature<BlsType>;
pub type BlsSecretKey = SecretKey<BlsType>;
pub type BlsAggregateSignature = AggregateSignature<BlsType>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Eq)]
pub struct BlsAggregateSignatureWrapper(pub BlsAggregateSignature);
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Eq)]
pub struct BlsSignatureWrapper(pub BlsSignature);

// TODO: This requires reparing
impl<'de> Deserialize<'de> for BlsAggregateSignatureWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize the map
        #[allow(non_snake_case)]
        #[derive(Deserialize)]
        struct MessageAugmentationWrapper {
            MessageAugmentation: String,
        }

        let wrapper = MessageAugmentationWrapper::deserialize(deserializer)?;

        // Parse the string into BlsSignature
        let key: G1Projective =
            serde_json::from_str(&format!("\"{}\"", wrapper.MessageAugmentation))
                .map_err(serde::de::Error::custom)?;

        Ok(BlsAggregateSignatureWrapper(
            BlsAggregateSignature::MessageAugmentation(key),
        ))
    }
}

impl Into<BlsAggregateSignature> for BlsAggregateSignatureWrapper {
    fn into(self) -> BlsAggregateSignature {
        self.0
    }
}

impl From<BlsAggregateSignature> for BlsAggregateSignatureWrapper {
    fn from(signature: BlsAggregateSignature) -> Self {
        BlsAggregateSignatureWrapper(signature)
    }
}

impl Into<BlsSignature> for BlsSignatureWrapper {
    fn into(self) -> BlsSignature {
        self.0
    }
}

impl From<BlsSignature> for BlsSignatureWrapper {
    fn from(signature: BlsSignature) -> Self {
        BlsSignatureWrapper(signature)
    }
}

impl<'de> Deserialize<'de> for BlsSignatureWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize the map
        #[allow(non_snake_case)]
        #[derive(Deserialize)]
        struct MessageAugmentationWrapper {
            MessageAugmentation: String,
        }

        let wrapper = MessageAugmentationWrapper::deserialize(deserializer)?;

        // Parse the string into BlsSignature
        let key: G1Projective =
            serde_json::from_str(&format!("\"{}\"", wrapper.MessageAugmentation))
                .map_err(serde::de::Error::custom)?;

        Ok(BlsSignatureWrapper(BlsSignature::MessageAugmentation(key)))
    }
}
