use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

struct ClusterNode {
    id: String,
    port: u16,
    child: Option<Child>,
    alive: bool,
}

struct LeaderInfo {
    id: String,
    port: u16,
    elapsed: Duration,
}

struct TestCluster {
    nodes: Vec<ClusterNode>,
    client: reqwest::Client,
}

impl TestCluster {
    async fn spawn(size: usize) -> Result<Self, Box<dyn Error>> {
        assert!(size > 0);

        std::fs::create_dir_all("states")?;

        let client = reqwest::Client::builder().no_proxy().build()?;
        let mut cluster = TestCluster {
            nodes: Vec::new(),
            client,
        };

        let ports = (0..size)
            .map(|_| allocate_port().expect("failed to allocate port"))
            .collect::<Vec<_>>();

        let base_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .to_string();

        let ids = (0..size)
            .map(|i| format!("test{}_{}", base_id, i))
            .collect::<Vec<_>>();

        for (index, id) in ids.iter().enumerate() {
            let peers = ports
                .iter()
                .enumerate()
                .filter(|(peer_idx, _)| *peer_idx != index)
                .map(|(_, port)| format!("127.0.0.1:{}", port))
                .collect::<Vec<_>>()
                .join(",");

            let state_path = format!("states/raft_state_{}.json", id);
            let _ = std::fs::remove_file(&state_path);

            let mut cmd = Command::new(env!("CARGO_BIN_EXE_raftlike"));
            cmd.args([
                "--id",
                id,
                "--port",
                &ports[index].to_string(),
                "--peers",
                &peers,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null());

            let child = cmd.spawn()?;

            cluster.nodes.push(ClusterNode {
                id: id.clone(),
                port: ports[index],
                child: Some(child),
                alive: true,
            });
        }

        cluster.wait_for_startup(Duration::from_secs(3)).await?;

        Ok(cluster)
    }

    async fn wait_for_startup(&self, timeout: Duration) -> Result<(), Box<dyn Error>> {
        let start = Instant::now();
        let mut pending: HashSet<u16> = self
            .nodes
            .iter()
            .filter(|node| node.alive)
            .map(|node| node.port)
            .collect();

        while start.elapsed() <= timeout {
            let ports: Vec<u16> = pending.iter().cloned().collect();

            for port in ports {
                if self.fetch_metrics(port).await.is_some() {
                    pending.remove(&port);
                }
            }

            if pending.is_empty() {
                return Ok(());
            }

            sleep(Duration::from_millis(50)).await;
        }

        Err("cluster failed to start within timeout".into())
    }

    async fn fetch_metrics(&self, port: u16) -> Option<Value> {
        let url = format!("http://127.0.0.1:{}/metrics", port);
        self.client
            .get(url)
            .send()
            .await
            .ok()?
            .json::<Value>()
            .await
            .ok()
    }

    async fn wait_for_leader(&self, timeout: Duration) -> Option<LeaderInfo> {
        let start = Instant::now();

        while start.elapsed() <= timeout {
            for node in self.nodes.iter().filter(|node| node.alive) {
                if let Some(metrics) = self.fetch_metrics(node.port).await {
                    if metrics.get("state").and_then(|state| state.as_str()) == Some("Leader") {
                        return Some(LeaderInfo {
                            id: node.id.clone(),
                            port: node.port,
                            elapsed: start.elapsed(),
                        });
                    }
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        None
    }

    async fn write_kv(&self, port: u16, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
        let url = format!("http://127.0.0.1:{}/kv", port);
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({ "key": key, "value": value }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("write failed with status {}", response.status()).into());
        }

        let json = response.json::<Value>().await?;
        if json.get("success").and_then(|success| success.as_bool()) == Some(true) {
            Ok(())
        } else {
            Err("leader did not confirm success".into())
        }
    }

    async fn read_kv(&self, port: u16, key: &str) -> Option<String> {
        let url = format!("http://127.0.0.1:{}/kv?key={}", port, key);
        self.client
            .get(url)
            .send()
            .await
            .ok()?
            .json::<Value>()
            .await
            .ok()
            .and_then(|json| json.get("value").cloned())
            .and_then(|value| value.as_str().map(|s| s.to_string()))
    }

    async fn wait_for_value(
        &self,
        key: &str,
        expected: &str,
        timeout: Duration,
    ) -> Result<(), Box<dyn Error>> {
        let start = Instant::now();

        while start.elapsed() <= timeout {
            let mut all_match = true;

            for node in self.nodes.iter().filter(|node| node.alive) {
                match self.read_kv(node.port, key).await {
                    Some(value) if value == expected => {}
                    _ => {
                        all_match = false;
                        break;
                    }
                }
            }

            if all_match {
                return Ok(());
            }

            sleep(Duration::from_millis(100)).await;
        }

        Err("value did not converge to expected value within timeout".into())
    }

    fn kill_node(&mut self, id: &str) -> Result<(), Box<dyn Error>> {
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id && node.alive)
        {
            if let Some(mut child) = node.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            node.alive = false;
            Ok(())
        } else {
            Err(format!("node {} not found or already offline", id).into())
        }
    }

    async fn assert_metrics_json(&self) -> Result<(), Box<dyn Error>> {
        for node in self.nodes.iter().filter(|node| node.alive) {
            let metrics = self
                .fetch_metrics(node.port)
                .await
                .ok_or_else(|| format!("metrics missing for node {}", node.id))?;

            let state = metrics
                .get("state")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("missing state for node {}", node.id))?;
            assert!(
                matches!(state, "Leader" | "Follower" | "Candidate"),
                "unexpected state {}",
                state
            );
            metrics
                .get("term")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| format!("missing term for node {}", node.id))?;
            metrics
                .get("commit_index")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| format!("missing commit_index for node {}", node.id))?;
            metrics
                .get("election_count")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| format!("missing election_count for node {}", node.id))?;
        }

        Ok(())
    }

    fn alive_ports(&self) -> Vec<u16> {
        self.nodes
            .iter()
            .filter(|node| node.alive)
            .map(|node| node.port)
            .collect()
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        for node in &mut self.nodes {
            if let Some(mut child) = node.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            node.alive = false;
        }
    }
}

fn allocate_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[tokio::test(flavor = "multi_thread")]
async fn election_completes_within_two_seconds() -> Result<(), Box<dyn Error>> {
    let cluster = TestCluster::spawn(3).await?;

    let leader = cluster
        .wait_for_leader(Duration::from_secs(2))
        .await
        .expect("leader not elected within 2 seconds");
    assert!(
        leader.elapsed <= Duration::from_secs(2),
        "leader election took {:?}",
        leader.elapsed
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replication_survives_leader_failure() -> Result<(), Box<dyn Error>> {
    let mut cluster = TestCluster::spawn(3).await?;

    let leader = cluster
        .wait_for_leader(Duration::from_secs(2))
        .await
        .expect("leader not elected within 2 seconds");

    cluster.write_kv(leader.port, "x", "1").await?;
    cluster
        .wait_for_value("x", "1", Duration::from_secs(2))
        .await?;

    cluster.kill_node(&leader.id)?;

    let new_leader = cluster
        .wait_for_leader(Duration::from_secs(4))
        .await
        .expect("no new leader elected after failure");
    assert_ne!(
        new_leader.id, leader.id,
        "new leader should differ from the killed node"
    );

    cluster
        .wait_for_value("x", "1", Duration::from_secs(2))
        .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn metrics_endpoint_returns_valid_json() -> Result<(), Box<dyn Error>> {
    let cluster = TestCluster::spawn(3).await?;

    cluster
        .wait_for_leader(Duration::from_secs(2))
        .await
        .expect("leader not elected within 2 seconds");
    cluster.assert_metrics_json().await?;

    let mut leader_found = false;
    for port in cluster.alive_ports() {
        let metrics = cluster
            .fetch_metrics(port)
            .await
            .expect("metrics endpoint unreachable");
        if metrics
            .get("state")
            .and_then(|state| state.as_str())
            == Some("Leader")
        {
            leader_found = true;
            break;
        }
    }

    assert!(leader_found, "no node reported itself as leader in metrics");

    Ok(())
}
