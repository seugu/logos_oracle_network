use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(about = "LON zone indexer")]
pub struct IndexerArgs {
    /// Logos blockchain node HTTP endpoint
    #[arg(
        long,
        default_value = "http://localhost:18080",
        env = "INDEXER_NODE_ENDPOINT"
    )]
    pub node_url: String,

    /// Logos blockchain node REST API endpoint
    #[arg(
        long,
        default_value = "http://localhost:18080",
        env = "SEQUENCER_REST_ENDPOINT"
    )]
    pub node_rest_url: String,

    /// Path to the channel ID file
    #[arg(long, default_value = "./data/channel.txt")]
    pub(crate) channel_path: String,

    /// Basic auth username for node endpoint
    #[arg(long, env = "INDEXER_NODE_AUTH_USERNAME")]
    pub node_auth_username: Option<String>,

    /// Basic auth password for node endpoint
    #[arg(long, env = "INDEXER_NODE_AUTH_PASSWORD")]
    pub node_auth_password: Option<String>,

    #[arg(long, default_value = "resources/register_contract_config.json")]
    pub register_contract_config: PathBuf,

    #[arg(long, default_value = "resources/prices_contract_config.json")]
    pub prices_contract_config: PathBuf,
}