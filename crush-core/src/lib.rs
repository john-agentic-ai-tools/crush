//! Crush Core Library
//!
//! High-performance parallel compression library.
//!
//! # Examples
//!
//! ```
//! use crush_core::hello;
//! assert_eq!(hello(), "Hello from crush-core!");
//! ```

/// Placeholder function demonstrating public API structure.
///
/// This function will be replaced with actual compression functionality
/// in future features. It exists to validate:
/// - Documentation builds correctly
/// - Public APIs are exported
/// - Tests can call public functions
///
/// # Examples
///
/// ```
/// use crush_core::hello;
/// assert_eq!(hello(), "Hello from crush-core!");
/// ```
#[must_use]
pub fn hello() -> &'static str {
    "Hello from crush-core!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        assert_eq!(hello(), "Hello from crush-core!");
    }
}
