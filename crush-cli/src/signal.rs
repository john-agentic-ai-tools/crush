use crush_core::cancel::{AtomicCancellationToken, CancellationToken};
use std::sync::Arc;

/// Setup Ctrl+C signal handler
///
/// Returns a cancellation token that will be set when Ctrl+C is pressed.
/// This token can be passed to compression operations for graceful cancellation.
pub fn setup_handler() -> Result<Arc<dyn CancellationToken>, ctrlc::Error> {
    let token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());
    let handler_token = token.clone();

    ctrlc::set_handler(move || {
        handler_token.cancel();
    })?;

    Ok(token)
}
