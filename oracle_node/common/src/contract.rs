use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PricesContractInfo {
    #[serde(with = "hex_u32_8")]
    pub oracle_prices_program_id: [u32; 8],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterContractInfo {
    #[serde(with = "hex_u32_8")]
    pub oracle_register_program_id: [u32; 8],
    // #[serde(with = "hex_32_bytes")]
    // pub oracle_node_id: [u8; 32],
    #[serde(with = "bs58_32_bytes")]
    pub oracle_register_account: [u8; 32],
    #[serde(with = "bs58_32_bytes")]
    pub oracle_node_funding_account: [u8; 32],
    #[serde(with = "bs58_32_bytes")]
    pub token_definition_account: [u8; 32],
}

pub mod hex_u32_8 {

    use serde::{Deserialize, Deserializer, Serializer};

    // Converts [u32; 8] -> 32 bytes -> hex string
    pub fn serialize<S>(array: &[u32; 8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = [0u8; 32];
        for (i, &val) in array.iter().enumerate() {
            bytes[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
        }
        let hex_string = hex::encode(bytes);
        serializer.serialize_str(&hex_string)
    }

    // Converts hex string -> 32 bytes -> [u32; 8]
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u32; 8], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        // TODO: Remove this?
        let clean_s = s.strip_prefix("0x").unwrap_or(&s);
        let decoded = hex::decode(clean_s).map_err(serde::de::Error::custom)?;

        if decoded.len() != 32 {
            return Err(serde::de::Error::custom(
                "Hex string must represent exactly 32 bytes (64 hex characters)",
            ));
        }

        let mut array = [0u32; 8];
        for (i, chunk) in decoded.chunks_exact(4).enumerate() {
            array[i] = u32::from_le_bytes(chunk.try_into().unwrap());
        }
        Ok(array)
    }
}

pub mod hex_32_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    // Converts the [u8; 32] array into a hex string for JSON
    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(bytes);
        serializer.serialize_str(&hex_string)
    }

    // Converts the JSON hex string back into a [u8; 32] array
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {

        let s: String = Deserialize::deserialize(deserializer)?;

        // TODO: remove this?
        let clean_s = s.strip_prefix("0x").unwrap_or(&s);

        // Decode the hex string into a Vec<u8>
        let decoded = hex::decode(clean_s).map_err(serde::de::Error::custom)?;

        // Ensure it is exactly 32 bytes and convert it to a fixed array
        decoded.try_into().map_err(|_| {
            serde::de::Error::custom("Hex string must represent exactly 32 bytes")
        })
    }
}

pub mod bs58_32_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = bs58::encode(bytes).into_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let decoded = bs58::decode(&s)
            .into_vec()
            .map_err(serde::de::Error::custom)?;

        decoded.try_into().map_err(|_| {
            serde::de::Error::custom("Base58 string must decode to exactly 32 bytes")
        })
    }
}
