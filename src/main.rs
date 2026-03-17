pub enum Node {
    Internal(Box<InternalNode>),
    External(Vec<u8>),
}

pub struct InternalNode {
    pub crit_bit: usize,
    pub left: Node,
    pub right: Node,
}

fn main() {
    println!("Hello, world!");
}
