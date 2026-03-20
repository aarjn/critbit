pub enum Node {
    Internal(Box<InternalNode>),
    External(Vec<u8>),
}

pub struct InternalNode {
    pub crit_bit: usize,
    pub left: Node,
    pub right: Node,
}

pub struct CritBitTree {
    root: Option<Node>,
}

impl CritBitTree {
    pub fn new() -> Self {
        CritBitTree { root: None }
    }
}

fn find_crit_bit(a: &[u8], b: &[u8]) -> Option<usize> {
    let min_len = a.len().min(b.len());

    // find the first differing byte
    for i in 0..min_len {
        if a[i] != b[i] {
            let xor = a[i] ^ b[i];
            let bit_in_bytes = 7 - xor.leading_zeros() as usize;
            return Some(i * 8 + bit_in_bytes);
        }
    }

    // keys differ only in length
    if a.len() != b.len() {
        Some(min_len * 8)
    } else {
        None
    }
}

fn main() {
    println!("Hello, world!");
}
