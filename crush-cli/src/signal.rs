use crush_core::cancel::{AtomicCancellationToken, CancellationToken};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Signal handler state containing both the high-level cancellation token
/// and a raw `AtomicBool` that can be passed to `decompress_with_cancel`.
pub struct SignalState {
    /// High-level cancellation token for CLI-level interrupt checks.
    pub token: Arc<dyn CancellationToken>,
    /// Raw cancel flag passed through to compression/decompression plugins.
    pub cancel_flag: Arc<AtomicBool>,
}

/// Setup Ctrl+C signal handler
///
/// Returns a `SignalState` containing both a `CancellationToken` (for CLI-level
/// checks) and a raw `Arc<AtomicBool>` (for passing into plugin decompression).
/// Both are set when Ctrl+C is pressed.
pub fn setup_handler() -> Result<SignalState, ctrlc::Error> {
    let token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let handler_token = token.clone();
    let handler_flag = cancel_flag.clone();

    ctrlc::set_handler(move || {
        handler_token.cancel();
        handler_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    })?;

    Ok(SignalState { token, cancel_flag })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crush_core::cancel::CancellationToken;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_signal_state_token_and_flag_independent() {
        // Verify that token and cancel_flag are independent until wired up
        let token: Arc<dyn CancellationToken> =
            Arc::new(crush_core::cancel::AtomicCancellationToken::new());
        let cancel_flag = Arc::new(AtomicBool::new(false));

        let state = SignalState {
            token: token.clone(),
            cancel_flag: cancel_flag.clone(),
        };

        assert!(!state.token.is_cancelled());
        assert!(!state.cancel_flag.load(Ordering::SeqCst));

        // Cancelling token doesn't affect flag
        state.token.cancel();
        assert!(state.token.is_cancelled());
        assert!(!state.cancel_flag.load(Ordering::SeqCst));

        // Setting flag doesn't affect token (already cancelled above, but
        // demonstrates they are independent)
        state.cancel_flag.store(true, Ordering::SeqCst);
        assert!(state.cancel_flag.load(Ordering::SeqCst));
    }
}
