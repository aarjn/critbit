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

fn main() {
    println!("Hello, world!");
}
