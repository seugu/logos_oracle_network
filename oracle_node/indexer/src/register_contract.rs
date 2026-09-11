use anyhow::{anyhow, Context};
use tracing::debug;
// third-party: lez
use spel_framework::prelude::ProgramId;
use wallet::WalletCore;
// internal
use oracle_register_client::{
    OracleRegisterClient,
    RegisterState as OracleRegisterState
};
use common::RegisterContractInfo;

pub async fn fetch_registered(rc_info: &RegisterContractInfo) -> anyhow::Result<Vec<[u8; 32]>> {

    let wallet_core = WalletCore::from_env().context("Getting wallet accounts from env")?;
    let oracle_register_program_id = ProgramId::from(rc_info.oracle_register_program_id);
    let client = OracleRegisterClient::new(&wallet_core, oracle_register_program_id);

    // debug!("Fetching oracle register state...");
    // debug!("client program id: {:?}", client.program_id);
    let register_store = client.fetch_register::<OracleRegisterState>().await
        .map_err(|err| anyhow!("{}", err))?;

    // debug!("register store fetched...");

    Ok(register_store.registered.to_vec())
}


