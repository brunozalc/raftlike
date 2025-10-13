mod node;

use node::RaftNode;

fn main() {
    let peers = vec![
        ("localhost".to_string(), 8081),
        ("localhost".to_string(), 8082),
    ];
    let raft_node = RaftNode::new("A".to_string(), 8080, peers);

    println!("Raft node created with ID: {}", raft_node.id());
}
