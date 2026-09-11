/// Generate IDL JSON for the oracle_prices program.
///
/// Usage:
///   cargo run --bin generate_idl > oracle_prices-idl.json

spel_framework::generate_idl!("../methods/guest/src/bin/oracle_prices.rs");
