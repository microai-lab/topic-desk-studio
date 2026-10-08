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
    #[error("翻译失败：{0}")]
    Translation(String),
}

/// Tauri commands serialize errors as strings without exposing internal backtraces.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_errors_are_not_reported_as_collection_failures() {
        assert_eq!(
            AppError::Translation("模型没有返回文本译文".into()).to_string(),
            "翻译失败：模型没有返回文本译文"
        );
    }
}
