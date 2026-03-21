//! Crit-bit tree — a binary trie keyed on byte slices.
//!
//! Convention (matches production usage):
//!   bit 0 = LSB of byte 0  (the least significant bit of the whole key)
//!   bit N = the (N % 8)-th bit from the LSB inside byte (N / 8)
//!
//! Both `crit_bit_pos` and `get_bit` use this same convention so the
//! round-trip is consistent: whatever index `crit_bit_pos` returns,
//! `get_bit` extracts exactly that bit.

// ---------------------------------------------------------------------------
// Node types
// ---------------------------------------------------------------------------

/// A node is either an internal branch or an external leaf.
pub enum Node {
    Internal(Box<InternalNode>),
    External(Vec<u8>),
}

/// An internal (branch) node.
/// `crit_bit` is the index of the first bit where the two subtrees differ.
/// `children[0]` is the subtree where that bit is 0,
/// `children[1]` is the subtree where that bit is 1.
pub struct InternalNode {
    pub crit_bit: usize,
    pub children: [Node; 2],
}

// ---------------------------------------------------------------------------
// The tree
// ---------------------------------------------------------------------------

pub struct CritBitTree {
    root: Option<Node>,
}

impl CritBitTree {
    pub fn new() -> Self {
        CritBitTree { root: None }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }
}

// ---------------------------------------------------------------------------
// Bit helpers  —  LSB-first convention
// ---------------------------------------------------------------------------

/// Returns the global bit index (LSB = 0) of the most-significant bit
/// where `a` and `b` first differ, or `None` if they are identical.
///
/// "Most significant" in LSB-first means the highest-numbered bit among
/// all differing bits, which corresponds to the most significant binary
/// position in normal arithmetic.
fn crit_bit_pos(a: &[u8], b: &[u8]) -> Option<usize> {
    let min_len = a.len().min(b.len());

    for i in 0..min_len {
        if a[i] != b[i] {
            let xor = a[i] ^ b[i];
            // `leading_zeros` counts from the MSB of the byte.
            // In LSB-first global indexing the MSB of byte i is:
            //   global = i * 8 + (7 - leading_zeros)
            // because bit 7 of the byte is bit (i*8 + 7) globally.
            let bit_in_byte = 7 - xor.leading_zeros() as usize;
            return Some(i * 8 + bit_in_byte);
        }
    }

    // Keys match up to `min_len`; if lengths differ the shorter key has
    // implicit zero bytes, so the first differing bit is just past it.
    if a.len() != b.len() {
        Some(min_len * 8)
    } else {
        None // identical keys
    }
}

/// Returns the value (0 or 1) of bit `bit_index` in `key`.
///
/// Uses LSB-first: bit 0 is the LSB of key[0], bit 7 is the MSB of key[0],
/// bit 8 is the LSB of key[1], etc.
#[inline(always)]
fn get_bit(key: &[u8], bit_index: usize) -> usize {
    let byte_index = bit_index / 8;
    if byte_index >= key.len() {
        return 0; // treat missing bytes as zero
    }
    let bit_in_byte = bit_index % 8; // 0 = LSB of this byte
    ((key[byte_index] >> bit_in_byte) & 1) as usize
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

impl CritBitTree {
    /// Returns `true` if `key` is present in the tree.
    pub fn contains(&self, key: &[u8]) -> bool {
        match &self.root {
            None => false,
            Some(root) => Self::walk(root, key) == key,
        }
    }

    /// Walks down the tree following the crit-bits of `key`, returning the
    /// leaf we land on.  The leaf may differ from `key` — callers must verify.
    fn walk<'a>(mut node: &'a Node, key: &[u8]) -> &'a [u8] {
        loop {
            match node {
                Node::External(k) => return k,
                Node::Internal(inner) => {
                    let bit = get_bit(key, inner.crit_bit);
                    node = &inner.children[bit];
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Insertion
// ---------------------------------------------------------------------------

impl CritBitTree {
    /// Inserts `key` into the tree.
    ///
    /// Returns `true` if the key was newly inserted, `false` if it was
    /// already present.
    pub fn insert(&mut self, key: Vec<u8>) -> bool {
        // Case 1: empty tree — the new leaf becomes the root directly.
        let Some(root) = self.root.take() else {
            self.root = Some(Node::External(key));
            return true;
        };

        // Case 2: find the best-candidate leaf and compute the critical bit.
        let best_leaf = Self::walk(&root, &key);
        let Some(cb) = crit_bit_pos(best_leaf, &key) else {
            // Key already exists.
            self.root = Some(root);
            return false;
        };

        // Which side of the new inner node does the new key go on?
        let new_key_side = get_bit(&key, cb);

        // Case 3: splice the new inner node into the correct position.
        // Inner nodes must be ordered so that higher crit_bit values sit
        // closer to the root (they represent more-significant distinctions).
        // We walk again from the root and stop the moment we see a node
        // whose crit_bit < cb — the new node belongs above it.
        self.root = Some(Self::insert_node(root, key, cb, new_key_side));
        true
    }

    fn insert_node(node: Node, key: Vec<u8>, cb: usize, new_key_side: usize) -> Node {
        match node {
            // Reached a leaf — wrap it and the new key in a fresh inner node.
            Node::External(_) => {
                let mut children = [Node::External(vec![]), Node::External(vec![])];
                children[new_key_side] = Node::External(key);
                children[1 - new_key_side] = node;
                // Dummy replacement above used a placeholder; rebuild properly:
                // (the array shuffle above is fine; children[1-side] holds `node`)
                Node::Internal(Box::new(InternalNode {
                    crit_bit: cb,
                    children,
                }))
            }

            Node::Internal(mut inner) => {
                if inner.crit_bit < cb {
                    // This inner node belongs *below* the new one — wrap it.
                    let mut children = [Node::External(vec![]), Node::External(vec![])];
                    children[new_key_side] = Node::External(key);
                    children[1 - new_key_side] = Node::Internal(inner);
                    Node::Internal(Box::new(InternalNode {
                        crit_bit: cb,
                        children,
                    }))
                } else {
                    // Go deeper on the side matching the new key's bit at this node's crit_bit.
                    let side = get_bit(&key, inner.crit_bit);

                    // Temporarily move the child out so we can recurse without borrow issues.
                    let child = core::mem::replace(
                        &mut inner.children[side],
                        Node::External(vec![]), // placeholder
                    );
                    inner.children[side] = Self::insert_node(child, key, cb, new_key_side);
                    Node::Internal(inner)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Deletion
// ---------------------------------------------------------------------------

impl CritBitTree {
    /// Removes `key` from the tree.
    ///
    /// Returns `true` if the key was found and removed, `false` otherwise.
    pub fn delete(&mut self, key: &[u8]) -> bool {
        let Some(root) = self.root.take() else {
            return false;
        };

        match Self::delete_node(root, key) {
            (new_root, true) => {
                self.root = new_root;
                true
            }
            (original, false) => {
                self.root = original;
                false
            }
        }
    }

    /// Returns `(new_subtree, found)`.
    ///
    /// When a leaf is deleted its parent inner node is replaced by the sibling,
    /// so the caller receives the replacement subtree directly.
    fn delete_node(node: Node, key: &[u8]) -> (Option<Node>, bool) {
        match node {
            Node::External(ref k) => {
                if k.as_slice() == key {
                    (None, true) // deleted; parent should use the sibling
                } else {
                    (Some(node), false) // wrong leaf
                }
            }

            Node::Internal(mut inner) => {
                let side = get_bit(key, inner.crit_bit);

                // Check if the direct child on `side` is the target leaf.
                match &inner.children[side] {
                    Node::External(k) if k.as_slice() == key => {
                        // Found it — promote the sibling to replace this inner node.
                        let sibling = core::mem::replace(
                            &mut inner.children[1 - side],
                            Node::External(vec![]),
                        );
                        (Some(sibling), true)
                    }
                    _ => {
                        // Go deeper.
                        let child =
                            core::mem::replace(&mut inner.children[side], Node::External(vec![]));
                        let (new_child, found) = Self::delete_node(child, key);
                        inner.children[side] = new_child.unwrap_or(Node::External(vec![]));
                        (Some(Node::Internal(inner)), found)
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Prefix iteration
// ---------------------------------------------------------------------------

impl CritBitTree {
    /// Returns all keys in the tree that start with `prefix`, in no
    /// guaranteed order.
    pub fn prefix_collect<'a>(&'a self, prefix: &[u8]) -> Vec<&'a [u8]> {
        let Some(root) = &self.root else {
            return vec![];
        };

        // Walk to the subtree that contains all keys with this prefix.
        // Once crit_bit exceeds the prefix length in bits, every leaf in
        // the current subtree shares the same prefix bits — collect them all.
        let mut node = root;
        loop {
            match node {
                Node::External(k) => {
                    if k.starts_with(prefix) {
                        return vec![k.as_slice()];
                    } else {
                        return vec![];
                    }
                }
                Node::Internal(inner) => {
                    if inner.crit_bit >= prefix.len() * 8 {
                        // All leaves in this subtree share the prefix bits.
                        let mut out = vec![];
                        Self::collect_all(node, &mut out);
                        out.retain(|k| k.starts_with(prefix));
                        return out;
                    }
                    let bit = get_bit(prefix, inner.crit_bit);
                    node = &inner.children[bit];
                }
            }
        }
    }

    /// Collects every leaf in `node`'s subtree into `out`.
    fn collect_all<'a>(node: &'a Node, out: &mut Vec<&'a [u8]>) {
        match node {
            Node::External(k) => out.push(k.as_slice()),
            Node::Internal(inner) => {
                Self::collect_all(&inner.children[0], out);
                Self::collect_all(&inner.children[1], out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- bit helpers --------------------------------------------------------

    #[test]
    fn get_bit_lsb_of_byte0() {
        // 0b00000001 — only the LSB is set, global bit index 0
        assert_eq!(get_bit(&[0b00000001], 0), 1);
        assert_eq!(get_bit(&[0b00000001], 1), 0);
    }

    #[test]
    fn get_bit_msb_of_byte0() {
        // 0b10000000 — only the MSB of byte 0 is set, global bit index 7
        assert_eq!(get_bit(&[0b10000000], 7), 1);
        assert_eq!(get_bit(&[0b10000000], 0), 0);
    }

    #[test]
    fn get_bit_second_byte() {
        // 0x00 0x01 — bit 8 (LSB of byte 1)
        assert_eq!(get_bit(&[0x00, 0x01], 8), 1);
        assert_eq!(get_bit(&[0x00, 0x01], 9), 0);
    }

    #[test]
    fn crit_bit_pos_lsb() {
        // 0 vs 1 differ at global bit 0 (LSB of byte 0)
        assert_eq!(crit_bit_pos(&[0u8], &[1u8]), Some(0));
    }

    #[test]
    fn crit_bit_pos_msb_of_byte0() {
        // 0x00 vs 0x80 differ at bit 7 (MSB of byte 0)
        assert_eq!(crit_bit_pos(&[0x00], &[0x80]), Some(7));
    }

    #[test]
    fn crit_bit_pos_byte_boundary() {
        // [0,0] vs [1,0] differ at bit 8 (LSB of byte 1)
        assert_eq!(crit_bit_pos(&[0x00, 0x00], &[0x00, 0x01]), Some(8));
    }

    #[test]
    fn crit_bit_pos_same_keys_returns_none() {
        assert_eq!(crit_bit_pos(b"abc", b"abc"), None);
    }

    #[test]
    fn crit_bit_roundtrip() {
        // The bit that crit_bit_pos identifies must be extractable by get_bit.
        let a = [0b10110100u8];
        let b = [0b10100100u8]; // differ at bit 4 (the 5th bit from LSB)
        let cb = crit_bit_pos(&a, &b).unwrap();
        assert_eq!(cb, 4);
        // In `a` that bit is 1; in `b` it is 0.
        assert_eq!(get_bit(&a, cb), 1);
        assert_eq!(get_bit(&b, cb), 0);
    }

    // --- empty tree ---------------------------------------------------------

    #[test]
    fn empty_tree_contains_nothing() {
        let t = CritBitTree::new();
        assert!(!t.contains(b"anything"));
    }

    #[test]
    fn empty_tree_delete_returns_false() {
        let mut t = CritBitTree::new();
        assert!(!t.delete(b"nope"));
    }

    #[test]
    fn empty_tree_prefix_collect_empty() {
        let t = CritBitTree::new();
        assert!(t.prefix_collect(b"x").is_empty());
    }

    // --- single-element tree ------------------------------------------------

    #[test]
    fn single_insert_contains() {
        let mut t = CritBitTree::new();
        assert!(t.insert(b"hello".to_vec()));
        assert!(t.contains(b"hello"));
        assert!(!t.contains(b"hell"));
        assert!(!t.contains(b"helloo"));
    }

    #[test]
    fn duplicate_insert_returns_false() {
        let mut t = CritBitTree::new();
        assert!(t.insert(b"x".to_vec()));
        assert!(!t.insert(b"x".to_vec()));
    }

    #[test]
    fn single_delete_empties_tree() {
        let mut t = CritBitTree::new();
        t.insert(b"only".to_vec());
        assert!(t.delete(b"only"));
        assert!(!t.contains(b"only"));
        assert!(t.is_empty());
    }

    // --- multi-element operations -------------------------------------------

    #[test]
    fn insert_multiple_all_present() {
        let mut t = CritBitTree::new();
        let keys: &[&[u8]] = &[b"apple", b"banana", b"cherry", b"date"];
        for k in keys {
            t.insert(k.to_vec());
        }
        for k in keys {
            assert!(t.contains(k), "{k:?} should be present");
        }
    }

    #[test]
    fn delete_one_others_remain() {
        let mut t = CritBitTree::new();
        t.insert(b"ant".to_vec());
        t.insert(b"bee".to_vec());
        t.insert(b"cat".to_vec());

        assert!(t.delete(b"bee"));
        assert!(!t.contains(b"bee"));
        assert!(t.contains(b"ant"));
        assert!(t.contains(b"cat"));
    }

    #[test]
    fn delete_absent_key_returns_false() {
        let mut t = CritBitTree::new();
        t.insert(b"foo".to_vec());
        assert!(!t.delete(b"bar"));
        assert!(t.contains(b"foo")); // unaffected
    }

    #[test]
    fn delete_all_one_by_one() {
        let keys: &[&[u8]] = &[b"one", b"two", b"three", b"four"];
        let mut t = CritBitTree::new();
        for k in keys {
            t.insert(k.to_vec());
        }
        for k in keys {
            assert!(t.delete(k));
        }
        assert!(t.is_empty());
        for k in keys {
            assert!(!t.contains(k));
        }
    }

    // --- prefix collect -----------------------------------------------------

    #[test]
    fn prefix_collect_basic() {
        let mut t = CritBitTree::new();
        t.insert(b"apple".to_vec());
        t.insert(b"application".to_vec());
        t.insert(b"apply".to_vec());
        t.insert(b"banana".to_vec());

        let mut results = t.prefix_collect(b"app");
        results.sort();
        assert_eq!(results, vec![b"apple".as_slice(), b"application", b"apply"]);
    }

    #[test]
    fn prefix_collect_exact_match_only() {
        let mut t = CritBitTree::new();
        t.insert(b"cat".to_vec());
        t.insert(b"catch".to_vec());
        t.insert(b"dog".to_vec());

        // Prefix "cat" matches "cat" and "catch"
        let mut results = t.prefix_collect(b"cat");
        results.sort();
        assert_eq!(results, vec![b"cat".as_slice(), b"catch"]);
    }

    #[test]
    fn prefix_collect_no_match_returns_empty() {
        let mut t = CritBitTree::new();
        t.insert(b"alpha".to_vec());
        t.insert(b"beta".to_vec());
        assert!(t.prefix_collect(b"gamma").is_empty());
    }

    #[test]
    fn prefix_collect_empty_prefix_returns_all() {
        let mut t = CritBitTree::new();
        let keys: &[&[u8]] = &[b"x", b"y", b"z"];
        for k in keys {
            t.insert(k.to_vec());
        }
        // Empty prefix matches every key.
        let mut results = t.prefix_collect(b"");
        results.sort();
        assert_eq!(results, vec![b"x".as_slice(), b"y", b"z"]);
    }

    // --- ordering and edge cases --------------------------------------------

    #[test]
    fn keys_differing_only_in_length() {
        let mut t = CritBitTree::new();
        t.insert(b"ab".to_vec());
        t.insert(b"abc".to_vec());
        assert!(t.contains(b"ab"));
        assert!(t.contains(b"abc"));
    }

    #[test]
    fn single_byte_keys() {
        let mut t = CritBitTree::new();
        for b in 0u8..=255 {
            t.insert(vec![b]);
        }
        for b in 0u8..=255 {
            assert!(t.contains(&[b]));
        }
        for b in 0u8..=255 {
            assert!(t.delete(&[b]));
        }
        assert!(t.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Demo main
// ---------------------------------------------------------------------------

fn main() {
    let mut tree = CritBitTree::new();

    // Insert some keys
    for key in [
        b"apple".as_slice(),
        b"application",
        b"apply",
        b"apt",
        b"banana",
        b"band",
    ] {
        tree.insert(key.to_vec());
    }

    // Point lookup
    println!("contains 'apple'       : {}", tree.contains(b"apple"));
    println!("contains 'app'         : {}", tree.contains(b"app"));
    println!("contains 'application' : {}", tree.contains(b"application"));

    // Prefix search
    let mut matches = tree.prefix_collect(b"app");
    matches.sort();
    let strs: Vec<_> = matches
        .iter()
        .map(|k| std::str::from_utf8(k).unwrap())
        .collect();
    println!("prefix 'app'           : {:?}", strs);

    let mut matches = tree.prefix_collect(b"ban");
    matches.sort();
    let strs: Vec<_> = matches
        .iter()
        .map(|k| std::str::from_utf8(k).unwrap())
        .collect();
    println!("prefix 'ban'           : {:?}", strs);

    // Delete and re-check
    tree.delete(b"apple");
    println!(
        "after delete 'apple'   : contains={}",
        tree.contains(b"apple")
    );

    let mut matches = tree.prefix_collect(b"app");
    matches.sort();
    let strs: Vec<_> = matches
        .iter()
        .map(|k| std::str::from_utf8(k).unwrap())
        .collect();
    println!("prefix 'app' after del : {:?}", strs);
}
