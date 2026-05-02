pub mod config;
pub mod setup;
pub mod skills;

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the HTTP proxy server (default when no subcommand is given)
    Serve(crate::cli::setup::ServeArgs),
    /// Setup integrations (opencode, mcp, config scaffold)
    Setup {
        #[command(subcommand)]
        action: SetupAction,
    },
    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Show configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetupAction {
    /// Write opencode provider config for this proxy
    Opencode(setup::SetupOpencodeArgs),
    /// Write MCP server config
    Mcp(setup::SetupMcpArgs),
    /// Scaffold ~/.config/oproxy/config.toml with defaults
    Config,
}

#[derive(Subcommand, Debug)]
pub enum SkillsAction {
    /// List all skills found in configured dirs
    List(skills::SkillsListArgs),
    /// Validate skill manifests in a directory
    Validate(skills::SkillsValidateArgs),
    /// Test skill selection for a message
    Test(skills::SkillsTestArgs),
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print the resolved config
    Show,
    /// Print the config file path
    Path,
}
