use thiserror::Error;

#[derive(Debug, Error)]
pub enum NexoscliError {
    #[error("Plugin '{0}' not found")]
    PluginNotFound(String),

    #[error("Command not recognized")]
    CommandNotFound,

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Other: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, NexoscliError>;
