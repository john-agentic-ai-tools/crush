use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Setup Ctrl+C signal handler
pub fn setup_handler() -> Result<Arc<AtomicBool>, ctrlc::Error> {
    let interrupted = Arc::new(AtomicBool::new(false));
    let r = interrupted.clone();

    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    })?;

    Ok(interrupted)
}
