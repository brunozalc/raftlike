use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser)]
#[command(name = "cli")]
#[command(about = "Raft cluster CLI client")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Put { key: String, value: String },
    Get { key: String },
    Status,
    Kill { node: String },
    Restart { node: String },
}

const NODES: &[&str] = &[
    "http://localhost:8080",
    "http://localhost:8081",
    "http://localhost:8082",
];

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Put { key, value } => {
            handle_put(&key, &value).await;
        }
        Commands::Get { key } => {
            handle_get(&key).await;
        }
        Commands::Status => {
            handle_status().await;
        }
        Commands::Kill { node } => {
            handle_kill(&node);
        }
        Commands::Restart { node } => {
            handle_restart(&node);
        }
    }
}

async fn handle_put(key: &str, value: &str) {
    let client = reqwest::Client::new();

    // try each node until we find the leader
    for node_url in NODES {
        let url = format!("{}/kv", node_url);

        let body = serde_json::json!({
            "key": key,
            "value": value
        });

        match client.post(&url).json(&body).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if json.get("success").and_then(|v| v.as_bool()) == Some(true) {
                            println!("✓ Set {}={}", key, value);
                            return;
                        } else {
                            // not leader or other error -> try next node
                            continue;
                        }
                    } else {
                        // malformed body -> try next node
                        continue;
                    }
                } else if response.status().as_u16() == 307 {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if let Some(leader_url) = json.get("leader_url") {
                            println!(
                                "✓ Redirecting to leader: {}",
                                leader_url.as_str().unwrap_or("Unknown")
                            );

                            let leader_body = serde_json::json!({
                                "key": key,
                                "value": value
                            });

                            if let Ok(leader_response) = client
                                .post(&format!("{}/kv", leader_url))
                                .json(&leader_body)
                                .send()
                                .await
                            {
                                if leader_response.status().is_success() {
                                    if let Ok(leader_json) =
                                        leader_response.json::<serde_json::Value>().await
                                    {
                                        if leader_json.get("success").and_then(|v| v.as_bool())
                                            == Some(true)
                                        {
                                            println!("✓ Set {}={}", key, value);
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    let text = response.text().await.unwrap_or_default();
                    if text.contains("Not the leader") {
                        continue;
                    }
                    eprintln!("Error: {}", text);
                    return;
                }
            }
            Err(_) => continue,
        }
    }

    eprintln!("✗ Could not reach any node");
}

async fn handle_get(key: &str) {
    let client = reqwest::Client::new();

    // try each node (followers can serve reads)
    for node_url in NODES {
        let url = format!("{}/kv?key={}", node_url, key);

        match client.get(&url).send().await {
            Ok(response) => {
                if let Ok(json) = response.json::<Value>().await {
                    if let Some(value) = json.get("value") {
                        if !value.is_null() {
                            println!("{}", value.as_str().unwrap_or(""));
                            return;
                        } else {
                            println!("(key not found)");
                            return;
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }

    eprintln!("✗ Could not reach any node");
}

async fn handle_status() {
    let client = reqwest::Client::new();

    println!("Cluster Status:\n");

    for (i, node_url) in NODES.iter().enumerate() {
        let node_id = ['A', 'B', 'C'][i];
        let url = format!("{}/metrics", node_url);

        match client.get(&url).send().await {
            Ok(response) => {
                if let Ok(json) = response.json::<Value>().await {
                    let state = json["state"].as_str().unwrap_or("Unknown");
                    let term = json["term"].as_u64().unwrap_or(0);
                    let commit = json["commit_index"].as_u64().unwrap_or(0);
                    let elections = json["election_count"].as_u64().unwrap_or(0);

                    let leader_mark = if state == "Leader" { " ★" } else { "" };

                    println!("Node {} ({}){}", node_id, node_url, leader_mark);
                    println!("  State: {}", state);
                    println!("  Term: {}", term);
                    println!("  Commit Index: {}", commit);
                    println!("  Elections: {}", elections);
                    println!();
                }
            }
            Err(_) => {
                println!("Node {} ({}) - Not responding", node_id, node_url);
                println!();
            }
        }
    }
}

fn handle_kill(node: &str) {
    let port = match node.to_uppercase().as_str() {
        "A" => 8080,
        "B" => 8081,
        "C" => 8082,
        _ => {
            eprintln!("✗ Invalid node. Use A, B, or C");
            return;
        }
    };

    // kill process on port
    let output = std::process::Command::new("lsof")
        .args(&["-ti", &format!(":{}", port)])
        .output();

    if let Ok(output) = output {
        let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !pid.is_empty() {
            std::process::Command::new("kill")
                .args(&["-9", &pid])
                .output()
                .ok();
            println!("✓ Killed Node {} (port {})", node.to_uppercase(), port);
        } else {
            println!("✗ Node {} not running", node.to_uppercase());
        }
    }
}

fn handle_restart(node: &str) {
    let (port, peers, id) = match node.to_uppercase().as_str() {
        "A" => (8080, "localhost:8081,localhost:8082", "A"),
        "B" => (8081, "localhost:8080,localhost:8082", "B"),
        "C" => (8082, "localhost:8080,localhost:8081", "C"),
        _ => {
            eprintln!("✗ Invalid node. Use A, B, or C");
            return;
        }
    };

    let log_file = format!("./logs/node_{}.log", id.to_lowercase());

    std::process::Command::new("./target/debug/raftlike")
        .args(&["--id", id, "--port", &port.to_string(), "--peers", peers])
        .stdout(std::fs::File::create(&log_file).unwrap())
        .stderr(std::fs::File::create(&log_file).unwrap())
        .spawn()
        .ok();

    println!("✓ Restarted Node {} (port {})", id, port);
    println!("  Log: tail -f {}", log_file);
}
