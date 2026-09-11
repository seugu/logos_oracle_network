#![no_main]
extern crate core;

use serde::{Deserialize, Serialize};
use spel_framework::prelude::*;
// use oracle_register_core::imt::{OracleMerkleTree, TREE_CAPACITY};
use oracle_register_core::RegisterState;
// use lee::program::Program;
use risc0_zkvm::serde::to_vec;
use sha2::{Sha256, Digest};
use token_core::{Instruction as TokenInstruction, TokenHolding};

risc0_zkvm::guest::entry!(main);

/*
/// Token Program Instruction.
#[derive(Serialize, Deserialize)]
pub enum TokenInstruction {
    Transfer { amount_to_transfer: u128 },
    NewFungibleDefinition {
        name: String,
        total_supply: u128,
        mint_authority: Option<AccountId>,
    },
    NewDefinitionWithMetadata {
        new_definition: String,
    },
    InitializeAccount,
    Burn { amount_to_burn: u128 },
    Mint { amount_to_mint: u128 },
    MintWithAuthority { amount_to_mint: u128 },
    PrintNft,
    SetAuthority { new_authority: Option<AccountId> },
    SetAuthorityWithAuthority { new_authority: Option<AccountId> },
}
*/

/*
/// The counter state stored on-chain.
///
/// `#[account_type]` registers this in the IDL so `spel inspect <PDA> --type CounterState`
/// can decode raw account bytes into readable JSON.
#[account_type]
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct RegisterState {
    // /// The current count value.
    // pub count: u64,
    /// The owner
    pub owner: [u8; 32],
    pub mtree: OracleMerkleTree,
    // Note: with tree depth of 10, this is 32 * 1024 -> 32Kb so ok
    pub registered: [[u8; 32]; TREE_CAPACITY],
}
*/

const LON_STAKE_AMOUNT: u128 = 3;

#[lez_program]
mod oracle_register {
    // use core::panic::PanicMessage;
    #[allow(unused_imports)]
    use super::*;

    /// Initialize the register contract with an owner.
    ///
    /// Creates a new PDA account derived from the literal seed "register".
    #[instruction]
    pub fn initialize(
        #[account(init, pda = literal("register"))]
        mut register: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        token_program_id: [u32; 8],
    ) -> SpelResult {
        let state = {
            let mut state = RegisterState::default();
            // TODO / FIXME: is owner required here?
            state.owner = *owner.account_id.value();
            state.token_program_id = token_program_id;
            state
        };
        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        register.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![register, owner], vec![]))
    }

    #[instruction]
    pub fn register(
        #[account(mut, pda = literal("register"))]
        mut register: AccountWithMetadata,
        // #[account(signer)]
        // owner: AccountWithMetadata,
        // Sender: An account with some LON TOKEN
        #[account(signer)]
        from: AccountWithMetadata,
        // Receiver: An escrow account owned by oracle_register contract (per registered oracle node)
        // TODO: better as it decouples the token sender from the oracle node registered
        // #[account(init, mut, pda = [literal("escrow"), arg("oracle_key")])]
        // #[account(init, mut, pda = [literal("escrow"), account("counter")])]
        #[account(mut)]
        mut to: AccountWithMetadata,
        #[account()]
        token_def_account: AccountWithMetadata,
        oracle_key: [u8; 32],
        pda_seed: [u8; 32],
    ) -> SpelResult {

        println!("[print] register instruction AA");
        eprintln!("[eprint] register");
        println!("oracle register pg id: {:?}", register.account.program_owner);

        let data: Vec<u8> = register.account.data.clone().into();
        let mut state: RegisterState = borsh::from_slice(&data).map_err(|e| {
            SpelError::DeserializationError {
                account_index: 0,
                message: e.to_string(),
            }
        })?;

        // Note: let everyone to register (for now, staking will be added later)
        /*
        if *owner.account_id.value() != state.owner {
            return Err(SpelError::Unauthorized {
                message: "Only the owner can increment".to_string(),
            });
        }
        */

        /*
        state.count = state.count.checked_add(amount).ok_or(SpelError::Overflow {
            operation: "counter increment".to_string(),
        })?;
        */

        // let pk = [47u8; 32];
        state.mtree.insert_oracle(oracle_key).map_err(|e| SpelError::Custom { code: 0, message: e.to_string() })?;
        let registered_idx = state.mtree.next_index.saturating_sub(1) as usize;
        println!("registered_idx: {}", registered_idx);
        state.registered[registered_idx] = oracle_key;

        println!("state registered len: {}", state.registered.len());
        println!("state registered 0: {:?}", state.registered[0]);

        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        println!("bytes len: {}", bytes.len());
        register.account.data = bytes.try_into().unwrap();

        // let token_pg_id = ProgramId::from([4266428645, 517024648, 1369049673, 1626402537, 3398049368, 2898630437, 1705650675, 3326128479]);
        let token_pg_id = ProgramId::from(state.token_program_id);
        assert_eq!(token_pg_id, token_def_account.account.program_owner);

        // let instruction_init = TokenInstruction::InitializeAccount {};
        // let instruction_data_init = to_vec(&instruction_init).unwrap();
        let instruction_data_init = to_vec(
            &TokenInstruction::InitializeAccount
        ).unwrap();

        println!("instruction_data_init: {:?}", instruction_data_init);
        println!("token def account pg id: {:?}", token_def_account.account.program_owner);
        println!("token def account is auth: {:?}", token_def_account.is_authorized);
        println!("to pg id: {:?}", to.account.program_owner);
        println!("to is authorized: {:?}", to.is_authorized);

        let to_authorized = {
            let mut to = to.clone();
            to.is_authorized = true;
            to
        };
        // let pda_seeds = vec![
        //     b"escrow".to_vec(),
        //     oracle_key.to_vec(),
        // ];

        // let pda_seed_ = PdaSeed::new(pda_seed);

        /*
        let pda_seed_ = {
            // 1. Emulate `literal("escrow")`: right-padded with zeroes to 32 bytes
            let mut literal_seed = [0u8; 32];
            literal_seed[..6].copy_from_slice(b"escrow");

            // 2. Emulate the macro's combination: SHA-256( literal_seed || oracle_key )
            let mut hasher = Sha256::new();
            hasher.update(literal_seed);
            hasher.update(oracle_key);
            let computed_seed_bytes: [u8; 32] = hasher.finalize().into();

            let pda_seed_ = PdaSeed::new(computed_seed_bytes);
            pda_seed_
        };
        println!("pda_seed_: {:?}", pda_seed_);
        */

        println!("pda_seed: {:?}", pda_seed);

        let chained_call_init = ChainedCall {
            // Call the token program
            program_id: token_pg_id,
            pre_states: vec![
                // definition account
                token_def_account.clone(),
                // account to init
                to_authorized.clone(),
                // to.clone()
            ],
            instruction_data: instruction_data_init,
            pda_seeds: vec![PdaSeed::new(pda_seed)],
        };

        // let instruction_data: InstructionData = vec![];
        let instruction_transfer = TokenInstruction::Transfer { amount_to_transfer: LON_STAKE_AMOUNT };
        let instruction_data_transfer = to_vec(&instruction_transfer).unwrap();
        println!("AAC instruction_data transfer: {:?}", instruction_data_transfer);

        /*
        account_sender: Ok(Account { program_owner: "e5884cfe882bd11e490a9a51e9eef060581e8aca2597c5acf329aa655fb140c6", balance: 0, data: Data([0, 9, 94, 216, 173, 42, 11, 199, 31, 8, 190, 170, 137, 128, 130, 23, 55, 149, 224, 140, 177, 12, 78, 4, 134, 50, 111, 45, 135, 109, 104, 238, 184, 140, 82, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), nonce: Nonce(2) })
        account_receiver: Ok(Account { program_owner: "e5884cfe882bd11e490a9a51e9eef060581e8aca2597c5acf329aa655fb140c6", balance: 0, data: Data([0, 9, 94, 216, 173, 42, 11, 199, 31, 8, 190, 170, 137, 128, 130, 23, 55, 149, 224, 140, 177, 12, 78, 4, 134, 50, 111, 45, 135, 109, 104, 238, 184, 140, 82, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), nonce: Nonce(2) })
         */
        let chained_call_transfer = ChainedCall {
            /// The program ID of the program to execute.
            program_id: token_pg_id,
            pre_states: vec![
                // Sender
                from.clone(),
                // Recipient
                to_authorized,
            ],
            instruction_data: instruction_data_transfer,
            pda_seeds: vec![PdaSeed::new(pda_seed)],
            /*
            pub pre_states: Vec<AccountWithMetadata>,
            /// The instruction data to pass.
            pub instruction_data: InstructionData,
            /// PDA seeds authorized for the callee. For each seed, the callee is authorized to
            /// mutate the `AccountId` derived from `(caller_program_id, seed)`, regardless of
            /// whether the account is public or private.
            pub pda_seeds: Vec<PdaSeed>,
            */
        };

        Ok(SpelOutput::execute(vec![register, from, to, token_def_account], vec![chained_call_transfer]))
    }

    /*
    /// Increment the counter by a given amount. Only the owner can increment.
    #[instruction]
    pub fn increment(
        #[account(mut, pda = literal("register"))]
        mut counter: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        amount: u64,
    ) -> SpelResult {
        let data: Vec<u8> = counter.account.data.clone().into();
        let mut state: RegisterState = borsh::from_slice(&data).map_err(|e| {
            SpelError::DeserializationError {
                account_index: 0,
                message: e.to_string(),
            }
        })?;

        if *owner.account_id.value() != state.owner {
            return Err(SpelError::Unauthorized {
                message: "Only the owner can increment".to_string(),
            });
        }

        state.count = state.count.checked_add(amount).ok_or(SpelError::Overflow {
            operation: "counter increment".to_string(),
        })?;

        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        counter.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![counter, owner], vec![]))
    }
    */

    /*
    /// Get the current count value (read-only).
    ///
    /// The caller inspects the counter account after the transaction to read the count —
    /// see Step 6 for the `spel inspect … --type CounterState` flow.
    #[instruction]
    pub fn get_register_state(
        #[account(pda = literal("register"))]
        counter: AccountWithMetadata,
    ) -> SpelResult {
        println!("[print] get_register_state");
        eprintln!("[eprint] get_register_state");
        // info!("[eprint] get_register_state");
        Ok(SpelOutput::execute(vec![counter], vec![]))
    }
    */

}