use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;

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
    port: u16,
    peers: Vec<(String, u16)>,

    // raft state
    role: NodeRole,
    current_term: u64,
    voted_for: Option<String>,
    log: Vec<LogEntry>,

    // volatile state (all nodes)
    commit_index: usize,
    last_applied: usize,
    kv_store: HashMap<String, String>,

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
    pub fn new(id: String, port: u16, peers: Vec<(String, u16)>) -> Self {
        let mut node = RaftNode {
            id,
            port,
            peers,
            role: NodeRole::Follower,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            kv_store: HashMap::new(),
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

    fn become_candidate(&mut self) {
        self.role = NodeRole::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id.clone());

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }
    }

    fn become_leader(&mut self) {
        self.role = NodeRole::Leader;
        self.leader_state = Some(LeaderState {
            next_index: vec![self.log.len(); self.peers.len()],
            match_index: vec![0; self.peers.len()],
        });
    }

    fn become_follower(&mut self, new_term: u64) {
        self.role = NodeRole::Follower;
        self.current_term = new_term;
        self.voted_for = None;
        self.leader_state = None;

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }
    }

    fn handle_vote_request(
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

    fn handle_append_entries(
        &mut self,
        leader_id: String,
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
        }

        if let Err(e) = self.save_state() {
            eprintln!("Warning: Could not save node state: {}", e);
        }

        true
    }

    fn save_state(&self) -> io::Result<()> {
        let state = PersistentState {
            current_term: self.current_term,
            voted_for: self.voted_for.clone(),
            log: self.log.clone(),
        };
        let json = serde_json::to_string_pretty(&state)?;
        let filename = format!("raft_state_{}.json", self.id);
        fs::write(filename, json)?;
        Ok(())
    }

    fn load_state(&mut self) -> io::Result<()> {
        let filename = format!("raft_state_{}.json", self.id);

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

    pub fn print_state(&self) {
        println!("Current term: {}", self.current_term);
        println!("Voted for: {:?}", self.voted_for);
        println!("Log entries: {}", self.log.len());
    }
}
