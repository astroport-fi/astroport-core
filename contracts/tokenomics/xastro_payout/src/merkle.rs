use sha2::{Digest, Sha256};

/// Leaf hash: sha256(0x00 || "{address}:{amount}"). The prefix keeps a leaf from ever passing
/// for an inner node.
pub fn leaf_hash(address: &str, amount: u128) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0u8]);
    h.update(format!("{address}:{amount}").as_bytes());
    h.finalize().into()
}

/// Inner node hash: sha256(0x01 || min(a, b) || max(a, b)). Sorting the pair means a proof is
/// just the list of siblings, without left/right flags.
pub fn node_hash(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update([1u8]);
    h.update(lo);
    h.update(hi);
    h.finalize().into()
}

/// Folds the leaf up through the proof and compares the result with the root.
pub fn verify(root: &[u8; 32], leaf: [u8; 32], proof: &[[u8; 32]]) -> bool {
    proof
        .iter()
        .fold(leaf, |acc, sibling| node_hash(&acc, sibling))
        == *root
}

/// Decodes a hex sha256 hash.
pub fn decode_hash(s: &str) -> Option<[u8; 32]> {
    hex::decode(s).ok()?.try_into().ok()
}
