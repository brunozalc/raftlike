use crate::api::{AppendRequest, AppendResponse, VoteRequest, VoteResponse};

pub async fn send_vote_request(
    peer_host: &str,
    peer_port: u16,
    request: VoteRequest,
) -> Result<VoteResponse, String> {
    let url = format!("http://{}:{}/vote", peer_host, peer_port);

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;

    response
        .json::<VoteResponse>()
        .await
        .map_err(|e| format!("Failed to parse: {}", e))
}

pub async fn send_append_entries(
    peer_host: &str,
    peer_port: u16,
    request: AppendRequest,
) -> Result<AppendResponse, String> {
    let url = format!("http://{}:{}/append", peer_host, peer_port);

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;

    response
        .json::<AppendResponse>()
        .await
        .map_err(|e| format!("Failed to parse: {}", e))
}
