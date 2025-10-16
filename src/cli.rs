use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "raft-node")]
#[command(about = "A Raft consensus node")]
pub struct Args {
    #[arg(long)]
    pub id: String,

    #[arg(long)]
    pub port: u16,

    #[arg(long)]
    pub peers: String, // comma-separated peers ("localhost:5001,localhost:5002,...")
}

impl Args {
    pub fn parse_peers(&self) -> Vec<(String, u16)> {
        self.peers
            .split(',')
            .filter(|s| !s.is_empty())
            .filter_map(|peer| {
                let parts: Vec<&str> = peer.split(':').collect();
                if parts.len() == 2 {
                    let host = parts[0].to_string();
                    let port = parts[1].parse::<u16>().ok()?;
                    Some((host, port))
                } else {
                    None
                }
            })
            .collect()
    }
}
