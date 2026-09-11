use borsh::{BorshDeserialize, BorshSerialize};
use spel_framework_macros::account_type;
use crate::imt::{OracleMerkleTree, TREE_CAPACITY};

#[account_type]
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct RegisterState {
    // /// The current count value.
    // pub count: u64,
    /// The owner
    pub owner: [u8; 32],
    pub token_program_id: [u32; 8],
    pub mtree: OracleMerkleTree,
    // Note: with tree depth of 10, this is 32 * 1024 -> 32Kb so ok
    pub registered: [[u8; 32]; TREE_CAPACITY],
}

impl Default for RegisterState {
    fn default() -> Self {
        Self {
            owner: [0; 32],
            token_program_id: [0; 8],
            mtree: Default::default(),
            registered: [[0u8; 32]; TREE_CAPACITY],
        }
    }
}