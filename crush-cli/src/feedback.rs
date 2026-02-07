//! User feedback messages for cancellation and progress indication

use is_terminal::IsTerminal;
use std::io::{self, Write};

/// Display a hint to the user that they can press Ctrl+C to cancel
///
/// This is shown for large file operations to inform users they can interrupt
/// the operation if needed.
///
/// # Arguments
///
/// * `show` - Whether to actually show the hint (based on file size, terminal type, etc.)
///
/// # Examples
///
/// ```no_run
/// use crush_cli::feedback;
///
/// // Show hint for large file operations
/// feedback::show_cancel_hint(true);
/// ```
pub fn show_cancel_hint(show: bool) {
    if !show {
        return;
    }

    // Only show hint if stderr is a terminal (not piped/redirected)
    if !std::io::stderr().is_terminal() {
        return;
    }

    // Write hint to stderr so it doesn't interfere with stdout data
    let _ = writeln!(io::stderr(), "ℹ️  Press Ctrl+C to cancel this operation");
}

/// Display immediate feedback that cancellation has been received
///
/// This is called when SIGINT/Ctrl+C is detected to provide instant user feedback
/// that their cancellation signal was received and is being processed.
///
/// # Examples
///
/// ```no_run
/// use crush_cli::feedback;
///
/// // Called when Ctrl+C is detected
/// feedback::show_cancelling_message();
/// ```
#[allow(dead_code)] // Part of public API, will be used for enhanced cancellation feedback
pub fn show_cancelling_message() {
    // Always show to stderr, regardless of terminal type
    // This is critical feedback that should always be visible
    let _ = writeln!(io::stderr(), "\nCancelling operation...");
    let _ = io::stderr().flush();
}

/// Helper to determine if cancel hint should be shown based on file size
///
/// Shows hint for files larger than 1MB to avoid cluttering output
/// for quick operations.
///
/// # Arguments
///
/// * `file_size` - Size of the input file in bytes
///
/// # Returns
///
/// `true` if the hint should be displayed, `false` otherwise
///
/// # Examples
///
/// ```
/// use crush_cli::feedback;
///
/// assert!(feedback::should_show_hint(2 * 1024 * 1024)); // 2MB - show hint
/// assert!(!feedback::should_show_hint(512 * 1024));     // 512KB - don't show
/// ```
pub fn should_show_hint(file_size: u64) -> bool {
    file_size > 1024 * 1024 // > 1MB
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_show_hint_for_large_files() {
        assert!(should_show_hint(2 * 1024 * 1024)); // 2MB
        assert!(should_show_hint(10 * 1024 * 1024)); // 10MB
    }

    #[test]
    fn test_should_not_show_hint_for_small_files() {
        assert!(!should_show_hint(512 * 1024)); // 512KB
        assert!(!should_show_hint(1024 * 1024)); // exactly 1MB
        assert!(!should_show_hint(100)); // 100 bytes
    }

    #[test]
    fn test_show_cancel_hint_does_not_panic() {
        // Just verify it doesn't panic when called
        show_cancel_hint(true);
        show_cancel_hint(false);
    }

    #[test]
    fn test_show_cancelling_message_does_not_panic() {
        // Just verify it doesn't panic when called
        show_cancelling_message();
    }
}
