use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;

use crate::error::AppError;

static LOGGING_INIT_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn init_logging(level: &str) -> Result<(), AppError> {
    let result = LOGGING_INIT_RESULT.get_or_init(|| {
        let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .try_init()
            .map_err(|error| error.to_string())
    });

    match result {
        Ok(()) => Ok(()),
        Err(error) => Err(AppError::LoggingInit(error.clone())),
    }
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;

    #[test]
    fn logging_init_smoke_test() -> Result<(), AppError> {
        super::init_logging("info")?;
        super::init_logging("debug")?;

        tracing::info!("logging smoke test");
        Ok(())
    }
}
