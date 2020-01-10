//! The configuration settings definitions.

use serde::Deserialize;

/// Settings for github login. To configure a github OAuth app must have been
/// provisioned.
#[derive(Debug, Deserialize)]
pub struct GithubSettings {
    /// The OAuth app "client ID"
    pub client_id: String,
    /// The OAuth app "client secret"
    pub client_secret: String,
    /// A random string used to authenticate requests from github. Can be any
    /// random secret value and can change.
    pub state: String,
    /// The github organization we require users to be a member of.
    pub required_org: String,
}

/// Setting for daemonization
#[derive(Debug, Deserialize)]
pub struct DaemonizeSettings {
    /// Where to store pid file when daemonizing
    pub pid_file: String,
}

/// Configuration settings for app
#[derive(Debug, Deserialize)]
pub struct Settings {
    /// Configures github login
    pub github: GithubSettings,
    /// Path for sqlite database.
    #[serde(default = "default_database_file")]
    pub database_file: String,
    /// Directory to look for the web resources
    #[serde(default = "default_resource_dir")]
    pub resource_dir: String,
    /// The web root prefix
    #[serde(default = "default_web_root")]
    pub web_root: String,
    /// Bind address for HTTP server
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Logging config
    #[serde(default)]
    pub log: sloggers::LoggerConfig,
    /// If and how to daemonize after start.
    pub daemonize: Option<DaemonizeSettings>,
}

// We set some defaults below. This seems to be the easiest way of doing it....

fn default_database_file() -> String {
    "shaft.db".to_string()
}

fn default_resource_dir() -> String {
    "res".to_string()
}

fn default_web_root() -> String {
    "/".to_string()
}

fn default_bind() -> String {
    "127.0.0.1:8975".to_string()
}
