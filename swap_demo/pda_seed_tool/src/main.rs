//! Prints the pool PDA (and its raw seed) for a token definition account.
//!
//! The address printed here is what you pass as `--pool-account`; the guest
//! recomputes the same value and rejects anything else. Both sides derive it
//! from `swap_demo_core`, so the tool cannot drift away from the contract.

use anyhow::{anyhow, Result};
use risc0_zkvm::compute_image_id;
use spel_framework::prelude::*;
use std::fs;
use swap_demo_core::{pool_seed, POOL_SEED_LITERAL};

const PROGRAM_BIN: &str = "methods/guest/target/riscv32im-risc0-zkvm-elf/docker/swap_demo.bin";

fn program_id_from_path(path: &str) -> Result<[u32; 8]> {
    let elf_bytes = fs::read(path)
        .map_err(|e| anyhow!("cannot read {path}: {e} — run `make build` first"))?;
    Ok(compute_image_id(&elf_bytes)
        .map_err(|e| anyhow!("cannot compute image id: {e}"))?
        .into())
}

fn u32_8_to_hex(array: &[u32; 8]) -> String {
    let mut bytes = [0u8; 32];
    for (i, &val) in array.iter().enumerate() {
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
    }
    hex::encode(bytes)
}

fn main() -> Result<()> {
    let Some(def_account_b58) = std::env::args().nth(1) else {
        eprintln!("usage: pda_seed_tool <TOKEN_DEFINITION_ACCOUNT_BASE58>");
        eprintln!("  prints the swap_demo pool PDA for that token definition");
        std::process::exit(2);
    };

    let def_account: [u8; 32] = bs58::decode(&def_account_b58)
        .into_vec()
        .map_err(|e| anyhow!("`{def_account_b58}` is not valid base58: {e}"))?
        .try_into()
        .map_err(|v: Vec<u8>| anyhow!("account id must be 32 bytes, got {}", v.len()))?;

    let program_id = program_id_from_path(PROGRAM_BIN)?;
    let seed = pool_seed(&def_account);
    // Single 32-byte seed here: `pool_seed` has already done the two-seed
    // SHA-256 combine that `compute_pda` would otherwise do internally.
    let pda = compute_pda(&program_id, &[&seed]);

    println!("swap_demo pool PDA");
    println!("  program bin : {PROGRAM_BIN}");
    println!("  program id  : {}", u32_8_to_hex(&program_id));
    println!("  seed literal: {POOL_SEED_LITERAL}");
    println!("  definition  : {def_account_b58}");
    println!("  pda seed    : {}", hex::encode(seed));
    println!("  pool account: {pda}");

    Ok(())
}
