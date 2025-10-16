mod api;
mod cli;
mod network;
mod node;
mod timers;

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};

use clap::Parser;
use cli::Args;
use node::RaftNode;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct KvQuery {
    key: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let peers = args.parse_peers();
    let port = args.port;
    let id = args.id.clone();

    println!("Starting raft node:");
    println!("  ID: {}", id);
    println!("  Port: {}", port);
    println!("  Peers: {:?}", peers);

    let raft_node = RaftNode::new(id, port, peers); // creates the actual raft node
    let shared_node = Arc::new(Mutex::new(raft_node)); // creates an arc pointer to the node data

    let node_for_election = shared_node.clone(); // arc pointer copy dedicated to the election
    let node_for_heartbeat = shared_node.clone(); // arc pointer copy dedicated to the heartbeat

    tokio::spawn(async move {
        timers::election_loop(node_for_election).await;
    }); // spawn an election thread with the repective election arc pointer

    tokio::spawn(async move {
        timers::heartbeat_loop(node_for_heartbeat).await;
    }); // spawn a heartbeat thread with the respective heartbeat arc pointer

    let app = Router::new()
        .route("/vote", post(handle_vote))
        .route("/append", post(handle_append))
        .route("/kv", post(handle_kv_write).get(handle_kv_read))
        .route("/metrics", get(handle_metrics))
        // ============= test endpoints =============
        .route("/test/become-leader", post(handle_test_become_leader))
        .route("/test/commit-all", post(handle_test_commit_all))
        .route("/test/debug", get(handle_test_debug))
        // ==========================================
        .with_state(shared_node.clone());

    let addr = format!("0.0.0.0:{}", port);
    println!("Raft node listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_vote(
    State(node): State<Arc<Mutex<RaftNode>>>,
    Json(req): Json<api::VoteRequest>,
) -> Json<api::VoteResponse> {
    let mut raft = node.lock().unwrap();

    let vote_granted = raft.handle_vote_request(
        req.candidate_id,
        req.candidate_term,
        req.last_log_index,
        req.last_log_term,
    );

    Json(api::VoteResponse {
        term: raft.current_term(),
        vote_granted,
    })
}

async fn handle_append(
    State(node): State<Arc<Mutex<RaftNode>>>,
    Json(req): Json<api::AppendRequest>,
) -> Json<api::AppendResponse> {
    let mut raft = node.lock().unwrap();

    let success = raft.handle_append_entries(
        req.leader_id,
        req.term,
        req.prev_log_index,
        req.prev_log_term,
        req.entries,
        req.leader_commit,
    );

    Json(api::AppendResponse {
        term: raft.current_term(),
        log_len: raft.log_len(),
        success,
    })
}

async fn handle_kv_write(
    State(node): State<Arc<Mutex<RaftNode>>>,
    Json(req): Json<api::KVWriteRequest>,
) -> Json<api::KVWriteResponse> {
    let mut raft = node.lock().unwrap();

    if !raft.is_leader() {
        return Json(api::KVWriteResponse {
            success: false,
            error: Some("Not the leader".to_string()),
        });
    }

    let command = format!("set {}={}", req.key, req.value);
    raft.append_to_log(command);

    Json(api::KVWriteResponse {
        success: true,
        error: None,
    })
}

async fn handle_kv_read(
    State(node): State<Arc<Mutex<RaftNode>>>,
    Query(params): Query<KvQuery>,
) -> Json<api::KVReadResponse> {
    let raft = node.lock().unwrap();

    let value = raft.get_value(&params.key);

    Json(api::KVReadResponse { value, error: None })
}

async fn handle_metrics(State(node): State<Arc<Mutex<RaftNode>>>) -> Json<api::MetricsResponse> {
    let raft = node.lock().unwrap();

    let response = api::MetricsResponse {
        state: format!("{:?}", raft.role()),
        term: raft.current_term(),
        commit_index: raft.commit_index(),
        election_count: raft.election_count(),
    };

    Json(response)
}

async fn handle_test_become_leader(State(node): State<Arc<Mutex<RaftNode>>>) -> &'static str {
    let mut raft = node.lock().unwrap();
    raft.force_become_leader();
    "Now leader"
}

async fn handle_test_commit_all(State(node): State<Arc<Mutex<RaftNode>>>) -> String {
    let mut raft = node.lock().unwrap();

    match raft.test_commit_all() {
        Ok(count) => format!("Committed {} entries", count),
        Err(e) => format!("Error: {}", e),
    }
}

async fn handle_test_debug(State(node): State<Arc<Mutex<RaftNode>>>) -> String {
    let raft = node.lock().unwrap();
    format!(
        "Role: {:?}\nTerm: {}\nLog length: {}\nCommit index: {}\nLast applied: {}\nLog: {}",
        raft.role(),
        raft.current_term(),
        raft.log_len(),
        raft.commit_index(),
        raft.last_applied(),
        raft.log_debug()
    )
}
