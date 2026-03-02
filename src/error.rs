use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("configuration parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("configuration file does not exist: {0}")]
    ConfigNotFound(PathBuf),

    #[error("logging initialization failed: {0}")]
    LoggingInit(String),

    #[error("command is not implemented yet: {0}")]
    NotImplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let app_error: AppError = io_error.into();

        assert!(matches!(app_error, AppError::Io(_)));
    }

    #[test]
    fn toml_error_conversion() {
        let parse_result = toml::from_str::<crate::config::Config>("this is not valid toml");

        let parse_error = match parse_result {
            Ok(_) => panic!("expected TOML parse to fail"),
            Err(error) => error,
        };

        let app_error: AppError = parse_error.into();
        assert!(matches!(app_error, AppError::ConfigParse(_)));
    }
}
