use crate::node::LogEntry;
use serde::{Deserialize, Serialize};

// POST /vote (request)
#[derive(Debug, Deserialize)]
pub struct VoteRequest {
    pub candidate_id: String,
    pub candidate_term: u64,
    pub last_log_index: usize,
    pub last_log_term: u64,
}

// POST /vote (response)
#[derive(Debug, Serialize)]
pub struct VoteResponse {
    pub term: u64,
    pub vote_granted: bool,
}

// POST /append (request)
#[derive(Debug, Deserialize)]
pub struct AppendRequest {
    pub leader_id: String,
    pub term: u64,
    pub prev_log_index: usize,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: usize,
}

// POST /append (response)
#[derive(Debug, Serialize)]
pub struct AppendResponse {
    pub term: u64,
    pub success: bool,
}

// POST /kv (request)
#[derive(Debug, Deserialize)]
pub struct KVWriteRequest {
    pub key: String,
    pub value: String,
}

// POST /kv (response)
#[derive(Debug, Serialize)]
pub struct KVWriteResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// GET /kv (response only)
#[derive(Debug, Serialize)]
pub struct KVReadResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// GET /metrics (response only)
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub state: String,
    pub term: u64,
    pub commit_index: usize,
    pub election_count: u64,
}
