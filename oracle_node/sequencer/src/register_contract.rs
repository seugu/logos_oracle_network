use anyhow::{anyhow, Context};
use sha2::{Sha256, Digest};
use tracing::{info, debug};
use spel_framework::prelude::{seed_from_str, AccountId, ProgramId, compute_pda_multi, ToSeed};
use wallet::WalletCore;
// internal
use oracle_register_client::{
    OracleRegisterClient,
    RegisterAccounts,
    RegisterState as OracleRegisterState
};
use common::RegisterContractInfo;

pub async fn sequencer_register(
    rc_info: RegisterContractInfo,
    node_id: &[u8; 32]
) -> anyhow::Result<()> {
    
    let wallet_core = WalletCore::from_env().context("Getting wallet accounts from env")?;
    let oracle_register_program_id = ProgramId::from(rc_info.oracle_register_program_id);
    let client = OracleRegisterClient::new(&wallet_core, oracle_register_program_id);

    debug!("Fetching oracle register state...");
    debug!("client program id: {:?}", client.program_id);
    let register_store = client.fetch_register::<OracleRegisterState>().await
        .map_err(|err| anyhow!("{}", err))?;

    debug!("register store fetched...");

    if !register_store.registered.contains(node_id) {
        info!("Oracle node (ID {:?}) not registered", node_id);
        info!("Registering...");

        let to_account = compute_vault_pda(&oracle_register_program_id, node_id);
        let to_account_pda_seed = vault_pda_seed_bytes(node_id);

        let accounts = RegisterAccounts {
            // Oracle register contract account
            register: AccountId::new(rc_info.oracle_register_account),
            // An account owned by the oracle node (holding tokens; that will be transferred for staking)
            from: AccountId::new(rc_info.oracle_node_funding_account),
            // The account was will receive the tokens (owned by oracle_register contract)
            // to: AccountId::new(rc_info.oracle_register_to),
            to: to_account,
            // The account holding the token definition
            token_def_account: AccountId::new(rc_info.token_definition_account),
        };

        let response = client.register(
            accounts,
            *node_id,
            // rc_info.oracle_register_to_pda_seed
            to_account_pda_seed
        )
            .await
            .map_err(|err| anyhow!("{}", err))?;

        debug!("Tx response: {}", response);

        info!("Waiting for confirmation...");
        let poller = wallet::poller::TxPoller::new(
            wallet_core.config(),
            wallet_core.sequencer_client.clone(),
        );

        let mut tx_hash = [0u8; 32];
        hex::decode_to_slice(response, &mut tx_hash)?;
        match poller.poll_tx(tx_hash.into()).await {
            Ok(_) => {
                debug!("Transaction confirmed — included in a block.")
            },
            Err(err) => {
                return Err(anyhow!("Transaction not confirmed: {}", err))
            },
        }

        info!("Register success!");
    } else {
        info!("Already registered, nothing to do...");
    }

    Ok(())
}

const ORACLE_REGISER_LITERAL: &str = "oracle_register__";

/// Raw seed bytes for the vault PDA, for inclusion in a `Register` instruction's `pda_seeds`.
/// Vault seed: [literal("oracle_register__"), arg("create_key")]
pub fn vault_pda_seed_bytes(create_key: &[u8; 32]) -> [u8; 32] {

    let tag = seed_from_str(ORACLE_REGISER_LITERAL);
    let mut hasher = Sha256::new();
    hasher.update(tag);
    hasher.update(create_key);
    hasher.finalize().into()
}

/// PDA for the multisig vault account.
pub fn compute_vault_pda(program_id: &ProgramId, create_key: &[u8; 32]) -> AccountId {
    let tag = seed_from_str(ORACLE_REGISER_LITERAL);
    compute_pda_multi(program_id, &[&tag as &dyn ToSeed, create_key])
}

