use thiserror::Error;

#[derive(Error, Debug)]
pub enum EnvxError {
    #[error("Environment variable not found: {0}")]
    VarNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid variable name: {0}")]
    InvalidVarName(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}
