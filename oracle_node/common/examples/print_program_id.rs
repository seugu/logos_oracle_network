use anyhow::anyhow;

fn main() -> anyhow::Result<()> {

    let hex_str_ = std::env::args().nth(1)
        .ok_or(anyhow::anyhow!("Expect 1 argument to be a hex string"))?;
    let mut hex_bytes = [0u8; 32];
    hex::decode_to_slice(hex_str_, &mut hex_bytes)
        .map_err(|_| anyhow!("Invalid hex string or wrong length (must be exactly 64 hex characters)"))?;

    println!("hex bytes: {:?}", hex_bytes);

    let mut u32_array = [0u32; 8];
    for (i, chunk) in hex_bytes.chunks_exact(4).enumerate() {
        // chunk.try_into().unwrap() safely turns &[u8] of length 4 into [u8; 4]
        u32_array[i] = u32::from_le_bytes(chunk.try_into().unwrap());
    }

    println!("hex words: {:?}", u32_array);

    Ok(())
}
