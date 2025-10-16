use crate::api;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum NodeRole {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug)]
pub struct LeaderState {
    next_index: Vec<usize>,
    match_index: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    term: u64,
    command: String,
}

pub struct RaftNode {
    // identity
    id: String,
    peers: Vec<(String, u16)>,

    // raft state
    role: NodeRole,
    current_term: u64,
    voted_for: Option<String>,
    log: Vec<LogEntry>,

    // volatile state (all nodes)
    commit_index: usize, // index of highest log entry known to be committed
    last_applied: usize, // index of highest log entry applied to state machine
    kv_store: HashMap<String, String>,
    election_count: u64,
    last_heartbeat: Instant,

    // leader state
    leader_state: Option<LeaderState>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentState {
    current_term: u64,
    voted_for: Option<String>,
    log: Vec<LogEntry>,
}

impl RaftNode {
    pub fn new(id: String, peers: Vec<(String, u16)>) -> Self {
        let mut node = RaftNode {
            id,
            peers,
            role: NodeRole::Follower,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            kv_store: HashMap::new(),
            election_count: 0,
            last_heartbeat: Instant::now(),
            leader_state: None,
        };

        if let Err(e) = node.load_state() {
            eprintln!("Warning: Could not load node state: {}", e);
        }

        if let Err(e) = node.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }

        node
    }

    pub fn id(&self) -> &String {
        &self.id
    }

    pub fn role(&self) -> &NodeRole {
        &self.role
    }

    pub fn is_leader(&self) -> bool {
        matches!(self.role, NodeRole::Leader)
    }

    pub fn get_peers(&self) -> &Vec<(String, u16)> {
        &self.peers
    }

    pub fn current_term(&self) -> u64 {
        self.current_term
    }

    pub fn commit_index(&self) -> usize {
        self.commit_index
    }

    pub fn election_count(&self) -> u64 {
        self.election_count
    }

    pub fn get_value(&self, key: &str) -> Option<String> {
        self.kv_store.get(key).cloned()
    }

    pub fn get_vote_request(&self) -> api::VoteRequest {
        let (last_log_index, last_log_term) = if self.log.is_empty() {
            (0, 0)
        } else {
            let idx = self.log.len() - 1;
            let term = self.log.last().map(|e| e.term).unwrap_or(0);
            (idx, term)
        };

        api::VoteRequest {
            candidate_id: self.id.clone(),
            candidate_term: self.current_term,
            last_log_index,
            last_log_term,
        }
    }

    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    pub fn last_applied(&self) -> usize {
        self.last_applied
    }

    pub fn log_debug(&self) -> String {
        format!("{:?}", self.log)
    }

    pub fn reset_election_timer(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    pub fn election_timeout_elapsed(&self) -> bool {
        self.last_heartbeat.elapsed() > Duration::from_millis(400)
    }

    pub fn become_candidate(&mut self) {
        self.role = NodeRole::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id.clone());
        self.election_count += 1;

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }
    }

    pub fn become_leader(&mut self) {
        self.role = NodeRole::Leader;
        self.leader_state = Some(LeaderState {
            next_index: vec![self.log.len(); self.peers.len()],
            match_index: vec![0; self.peers.len()],
        });
    }

    pub fn become_follower(&mut self, new_term: u64) {
        self.role = NodeRole::Follower;
        self.current_term = new_term;
        self.voted_for = None;
        self.leader_state = None;

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }
    }

    pub fn handle_vote_request(
        &mut self,
        candidate_id: String,
        candidate_term: u64,
        candidate_last_log_index: usize,
        candidate_last_log_term: u64,
    ) -> bool {
        // check if candidate's term is outdated
        if candidate_term < self.current_term {
            return false;
        }

        // if candidate has higher term, update ourselves
        if candidate_term > self.current_term {
            self.become_follower(candidate_term);
        }

        // check if we already voted for someone else this term
        if let Some(ref voted_id) = self.voted_for {
            if voted_id != &candidate_id {
                return false;
            }
        }

        // check if candidate's log is at least as up-to-date
        if let Some(last) = self.log.last() {
            if candidate_last_log_term < last.term
                || (candidate_last_log_term == last.term
                    && candidate_last_log_index < self.log.len() - 1)
            {
                return false;
            }
        }

        // grant vote
        self.voted_for = Some(candidate_id);

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }

        true
    }

    pub fn handle_append_entries(
        &mut self,
        _leader_id: String,
        leader_term: u64,
        prev_log_idx: usize,
        prev_log_term: u64,
        entries: Vec<LogEntry>,
        leader_commit_index: usize,
    ) -> bool {
        // check if leader's term is outdated
        if leader_term < self.current_term {
            return false;
        } else {
            self.become_follower(leader_term);
            self.reset_election_timer();
        }

        // verify prev_log_idx and prev_log_term
        if prev_log_idx > 0 {
            if let Some(entry) = self.log.get(prev_log_idx) {
                if entry.term != prev_log_term {
                    return false;
                }
            } else {
                return false;
            }
        }

        // append entries
        for (idx, entry) in entries.into_iter().enumerate() {
            let log_index = prev_log_idx + idx + 1;
            if log_index < self.log.len() {
                if self.log[log_index].term != entry.term {
                    self.log.truncate(log_index);
                    self.log.push(entry);
                }
            } else {
                self.log.push(entry);
            }
        }

        // update commit_index
        if leader_commit_index > self.commit_index {
            self.commit_index = leader_commit_index.min(self.log.len());
            self.apply_committed_entries();
        }

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }

        true
    }

    pub fn handle_append_response(
        &mut self,
        peer_idx: usize,
        success: bool,
        follower_log_len: usize,
    ) {
        if let Some(ref mut leader_state) = self.leader_state {
            if success {
                leader_state.match_index[peer_idx] = follower_log_len.saturating_sub(1);
                leader_state.next_index[peer_idx] = follower_log_len;
            } else {
                if leader_state.next_index[peer_idx] > 0 {
                    leader_state.next_index[peer_idx] -= 1;
                }
            }
        }
    }

    pub fn apply_committed_entries(&mut self) {
        while self.last_applied < self.commit_index {
            if let Some(entry) = self.log.get(self.last_applied) {
                if let Some(kv) = entry.command.strip_prefix("set ") {
                    if let Some((key, value)) = kv.split_once('=') {
                        self.kv_store.insert(key.to_string(), value.to_string());
                    }
                }
            }
            self.last_applied += 1;
        }
    }

    pub fn get_append_entries_for_peer(&self, peer_idx: usize) -> api::AppendRequest {
        let next_idx = if let Some(ref leader_state) = self.leader_state {
            leader_state.next_index[peer_idx]
        } else {
            self.log.len()
        };

        let prev_log_index = if next_idx > 0 { next_idx - 1 } else { 0 };

        let prev_log_term = if prev_log_index > 0 && prev_log_index <= self.log.len() {
            self.log
                .get(prev_log_index - 1)
                .map(|e| e.term)
                .unwrap_or(0)
        } else {
            0
        };

        let entries: Vec<LogEntry> = self.log[next_idx.min(self.log.len())..]
            .iter()
            .cloned()
            .collect();

        api::AppendRequest {
            leader_id: self.id.clone(),
            term: self.current_term,
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit: self.commit_index,
        }
    }

    pub fn append_to_log(&mut self, command: String) {
        let entry = LogEntry {
            term: self.current_term,
            command,
        };
        self.log.push(entry);
        let _ = self.save_state();
    }

    fn save_state(&self) -> io::Result<()> {
        let state = PersistentState {
            current_term: self.current_term,
            voted_for: self.voted_for.clone(),
            log: self.log.clone(),
        };
        let json = serde_json::to_string_pretty(&state)?;

        // create states folder at root if non-existent
        fs::create_dir_all("./states")?;

        let filename = format!("./states/raft_state_{}.json", self.id);
        fs::write(filename, json)?;
        Ok(())
    }

    fn load_state(&mut self) -> io::Result<()> {
        let filename = format!("./states/raft_state_{}.json", self.id);

        if !std::path::Path::new(&filename).exists() {
            // first startup, nothing to load
            return Ok(());
        }

        let json = fs::read_to_string(&filename)?;

        let state: PersistentState = serde_json::from_str(&json)?;

        self.current_term = state.current_term;
        self.voted_for = state.voted_for;
        self.log = state.log;

        Ok(())
    }

    pub fn force_become_leader(&mut self) {
        self.become_leader();
    }

    pub fn test_commit_all(&mut self) -> Result<usize, String> {
        if !self.is_leader() {
            return Err("Only leaders can commit entries".to_string());
        }

        self.commit_index = self.log.len();
        self.apply_committed_entries();
        Ok(self.commit_index)
    }
}
