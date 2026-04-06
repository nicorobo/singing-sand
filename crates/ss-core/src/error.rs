use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("audio engine error: {0}")]
    Audio(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
