//! Error boundary shared by database, command and future collection services.

use thiserror::Error;

/// Application errors are converted to safe user-facing strings at the Tauri boundary.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库操作失败：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("文件系统操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("应用状态锁已损坏")]
    PoisonedState,
    #[error("输入无效：{0}")]
    InvalidInput(String),
    #[error("凭据处理失败：{0}")]
    Credential(String),
    #[error("应用初始化失败：{0}")]
    Initialization(String),
    #[error("采集失败：{0}")]
    Collection(String),
}

/// Tauri commands serialize errors as strings without exposing internal backtraces.
pub type AppResult<T> = Result<T, AppError>;
