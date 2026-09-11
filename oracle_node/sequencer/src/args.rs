use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(about = "LON zone sequencer")]
pub struct SequencerArgs {
    /// Logos blockchain node HTTP endpoint
    #[arg(
        long,
        default_value = "http://localhost:18080",
        env = "SEQUENCER_NODE_ENDPOINT"
    )]
    pub node_url: String,

    /// Logos blockchain node REST API endpoint
    #[arg(
        long,
        default_value = "http://localhost:18080",
        env = "SEQUENCER_REST_ENDPOINT"
    )]
    pub node_rest_url: String,

    /// Path to the signing key file (created if it doesn't exist)
    #[arg(
        long,
        default_value = "./data",
        env = "SEQUENCER_DATA_FOLDER"
    )]
    pub data_folder: PathBuf,

    /// Path to the signing key file (created if it doesn't exist)
    #[arg(
        long,
        default_value = "sequencer.key",
        env = "SEQUENCER_SIGNING_KEY_PATH"
    )]
    pub key_path: PathBuf,

    // /// Path to the signing key file (created if it doesn't exist)
    // #[arg(
    //     long,
    //     default_value = "oracle.key",
    //     env = "SEQUENCER_ORACLE_SIGNING_KEY_PATH"
    // )]
    // pub oracle_key_path: PathBuf,

    /// Basic auth username for node endpoint
    #[arg(long, env = "SEQUENCER_NODE_AUTH_USERNAME")]
    pub node_auth_username: Option<String>,

    /// Basic auth password for node endpoint
    #[arg(long, env = "SEQUENCER_NODE_AUTH_PASSWORD")]
    pub node_auth_password: Option<String>,

    /// Path to the checkpoint file for crash recovery
    #[arg(
        long,
        default_value = "sequencer.checkpoint",
        env = "CHECKPOINT_PATH"
    )]
    pub checkpoint_path: PathBuf,

    /// Path to the channel ID file
    #[arg(long, default_value = "channel.txt", env = "CHANNEL_PATH")]
    pub channel_path: PathBuf,

    #[arg(long, help = "Pyth network bearer - optional")]
    pub(crate) pyth_bearer: Option<String>,

    #[arg(long, default_value = "resources/provider_config.json")]
    pub provider_config: PathBuf,
    
    #[arg(long, default_value = "resources/register_contract_config.json")]
    pub register_contract_config: PathBuf,
}