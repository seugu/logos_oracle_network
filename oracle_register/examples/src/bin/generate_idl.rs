/// Generate IDL JSON for the oracle_register program.
///
/// Usage:
///   cargo run --bin generate_idl > oracle_register-idl.json

spel_framework::generate_idl!("../methods/guest/src/bin/oracle_register.rs");
