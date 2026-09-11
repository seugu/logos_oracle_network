use borsh::{BorshDeserialize, BorshSerialize};
use tiny_keccak::{Hasher, Keccak};

pub const TREE_DEPTH: usize = 10;
/// Maximum number of leaves the tree can hold (2^TREE_DEPTH).
pub const TREE_CAPACITY: usize = 1 << TREE_DEPTH;

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
/// A lightweight Incremental Merkle Tree
pub struct OracleMerkleTree {
    /// Caches the right-most nodes at each level of the tree.
    pub filled_subtrees: [[u8; 32]; TREE_DEPTH],
    /// Pre-computed hashes of empty subtrees to calculate the root on the fly.
    pub zero_hashes: [[u8; 32]; TREE_DEPTH],
    /// The current, officially updated membership root.
    pub current_root: [u8; 32],
    /// The total number of registered oracles - acts as the insert index
    pub next_index: u32,
}

impl OracleMerkleTree {
    /// Initializes a new, empty tree.
    pub fn new() -> Self {
        let mut zero_hashes = [[0u8; 32]; TREE_DEPTH];
        let mut current_zero = [0u8; 32];
        // Pre-compute the zero hashes for each level of the tree
        for level in 0..TREE_DEPTH {
            zero_hashes[level] = current_zero;
            current_zero = hash_nodes(&current_zero, &current_zero);
        }
        Self {
            filled_subtrees: [[0u8; 32]; TREE_DEPTH],
            current_root: current_zero, // The root of an empty tree
            zero_hashes,
            next_index: 0,
        }
    }

    /// Appends a new oracle public key (leaf) and dynamically computes the new root.
    pub fn insert_oracle(&mut self, leaf: [u8; 32]) -> Result<(), &'static str> {
        if self.next_index >= (1 << TREE_DEPTH) {
            return Err("Merkle tree capacity reached");
        }

        let mut current_node = leaf;
        let mut index = self.next_index;

        // Bubble up the tree
        for level in 0..TREE_DEPTH {
            if index % 2 == 0 {
                // We are a left child. Store ourselves in the frontier.
                self.filled_subtrees[level] = current_node;

                // Since there is no right child yet, we compute the root from here
                // using the pre-computed zero hashes.
                let mut root = current_node;
                let mut root_index = index;

                for l in level..TREE_DEPTH {
                    if root_index % 2 == 0 {
                        root = hash_nodes(&root, &self.zero_hashes[l]);
                    } else {
                        root = hash_nodes(&self.filled_subtrees[l], &root);
                    }
                    root_index /= 2;
                }

                self.current_root = root;
                self.next_index += 1;
                return Ok(());
            } else {
                // We are a right child. Grab the left child from the frontier and hash them.
                let left_node = self.filled_subtrees[level];
                current_node = hash_nodes(&left_node, &current_node);
            }
            index /= 2;
        }

        // If the tree is completely full (e.g., node 1,048,575)
        self.current_root = current_node;
        self.next_index += 1;
        Ok(())
    }

    /// Returns the maximum capacity of the tree
    pub const fn capacity() -> usize {
        TREE_CAPACITY
    }

    /// Returns the number of currently registered oracle leaves.
    pub fn len(&self) -> usize {
        self.next_index as usize
    }
}

fn hash_nodes(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    hasher.update(left);
    hasher.update(right);

    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    output
}

impl Default for OracleMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_merkle::{Hasher, MerkleTree};
    use tiny_keccak::{Hasher as _, Keccak};

    #[derive(Clone)]
    pub struct Keccak256Hasher;

    impl Hasher for Keccak256Hasher {
        type Hash = [u8; 32];

        fn hash(data: &[u8]) -> [u8; 32] {
            let mut hasher = Keccak::v256();
            hasher.update(data);
            let mut output = [0u8; 32];
            hasher.finalize(&mut output);
            output
        }
    }

    /// Helper function to build a full `rs_merkle` tree padded with zero-leaves
    /// up to capacity 2^DEPTH; return the tree root
    fn compute_reference_root(inserted_leaves: &[[u8; 32]]) -> [u8; 32] {
        let total_capacity = 1 << TREE_DEPTH; // 2^TREE_DEPTH
        assert!(
            inserted_leaves.len() <= total_capacity,
            "Leaves exceed test tree capacity"
        );

        let mut full_leaves = vec![[0u8; 32]; total_capacity];
        for (i, leaf) in inserted_leaves.iter().enumerate() {
            full_leaves[i] = *leaf;
        }
        let reference_tree = MerkleTree::<Keccak256Hasher>::from_leaves(&full_leaves);
        reference_tree
            .root()
            .expect("Reference tree root calculation failed")
    }

    #[test]
    fn test_empty_tree_root_matches_reference() {
        let imt = OracleMerkleTree::new();
        let reference_root = compute_reference_root(&[]);

        assert_eq!(
            imt.current_root, reference_root,
            "Empty IMT root does not match reference root"
        );
    }

    #[test]
    fn test_single_leaf_matches_reference() {
        let mut imt = OracleMerkleTree::new();

        let mut sample_leaf = [0u8; 32];
        sample_leaf[0] = 42;

        imt.insert_oracle(sample_leaf).unwrap();

        let reference_root = compute_reference_root(&[sample_leaf]);

        assert_eq!(
            imt.current_root, reference_root,
            "Single-leaf IMT root does not match reference root"
        );
    }

    #[test]
    fn test_incremental_insertions_match_reference_at_every_step() {
        let mut imt = OracleMerkleTree::new();
        let mut inserted_leaves = Vec::new();

        for i in 1..=25 {
            let mut sample_leaf = [0u8; 32];
            sample_leaf[0] = i as u8;
            sample_leaf[31] = (i * 2) as u8;

            imt.insert_oracle(sample_leaf).unwrap();
            inserted_leaves.push(sample_leaf);

            let reference_root = compute_reference_root(&inserted_leaves);

            assert_eq!(
                imt.current_root, reference_root,
                "Root mismatch at insertion step {}", i
            );
        }
    }

    #[test]
    fn test_power_of_two_boundary_insertions() {
        let mut imt = OracleMerkleTree::new();
        let mut inserted_leaves = Vec::new();

        for i in 0..16 {
            let mut sample_leaf = [0u8; 32];
            sample_leaf[0] = (i + 10) as u8;

            imt.insert_oracle(sample_leaf).unwrap();
            inserted_leaves.push(sample_leaf);

            let count = inserted_leaves.len();
            if count.is_power_of_two() {
                let reference_root = compute_reference_root(&inserted_leaves);
                assert_eq!(
                    imt.current_root, reference_root,
                    "Root mismatch at power-of-two size {}", count
                );
            }
        }
    }

}