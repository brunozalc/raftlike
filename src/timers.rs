use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use tokio::time::sleep;

use crate::network;
use crate::node::RaftNode;

pub async fn election_loop(node: Arc<Mutex<RaftNode>>) {
    loop {
        let timeout_ms = rand::rng().random_range(300..500);
        let duration = Duration::from_millis(timeout_ms);

        println!("[ELECTION] Waiting {}ms", timeout_ms);
        sleep(duration).await;

        let mut raft = node.lock().unwrap();

        if !raft.is_leader() && raft.election_timeout_elapsed() {
            println!(
                "[ELECTION] Timeout expired! Starting election (term {})",
                raft.current_term() + 1
            );
            raft.become_candidate();
            raft.reset_election_timer();

            // prepare vote request
            let peers = raft.get_peers().clone();
            let vote_request = raft.get_vote_request();
            let current_term = raft.current_term();
            let node_id = raft.id().clone();

            drop(raft); // release lock before network calls

            // send vote requests to all
            println!("[ELECTION] Requesting votes from {} peers", peers.len());

            let node_clone = node.clone();
            tokio::spawn(async move {
                let mut votes_received = 1; // vote for self
                let majority = (peers.len() + 1) / 2 + 1;

                for (host, port) in peers {
                    let req = vote_request.clone();

                    match network::send_vote_request(&host, port, req).await {
                        Ok(response) => {
                            println!(
                                "[ELECTION] Peer {}:{} voted: {}",
                                host, port, response.vote_granted
                            );

                            if response.vote_granted {
                                votes_received += 1;

                                if votes_received >= majority {
                                    println!(
                                        "[ELECTION] WON ELECTION with {} votes!",
                                        votes_received
                                    );
                                    let mut raft = node_clone.lock().unwrap();

                                    if raft.current_term() == current_term {
                                        raft.become_leader();
                                    }
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            println!("[ELECTION] Failed to reach {}:{} - {}", host, port, e);
                        }
                    }
                }
            });
        } else {
            drop(raft);
        }
    }
}

pub async fn heartbeat_loop(node: Arc<Mutex<RaftNode>>) {
    loop {
        sleep(Duration::from_millis(100)).await;

        let (is_leader, peers, request) = {
            let raft = node.lock().unwrap();

            if !raft.is_leader() {
                drop(raft);
                continue;
            }

            let peers = raft.get_peers().clone();
            let request = raft.get_append_entries_request();

            (true, peers, request)
        }; // lock released here

        println!(
            "[HEARTBEAT] Sending heartbeats to {} followers...",
            peers.len()
        );

        for (host, port) in peers {
            let req = request.clone();

            // spawn a task for each peer (send in parallel)
            tokio::spawn(async move {
                match network::send_append_entries(&host, port, req).await {
                    Ok(response) => {
                        println!(
                            "[HEARTBEAT] Peer {}:{} responded: success={}",
                            host, port, response.success
                        );
                    }
                    Err(e) => {
                        println!("[HEARTBEAT] Failed to reach {}:{} - {}", host, port, e);
                    }
                }
            });
        }
    }
}
