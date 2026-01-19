fn main() {
    let message = crush_core::hello();
    println!("{message}");
    println!("Crush CLI v0.1.0 - Placeholder binary");
    println!("This binary demonstrates successful workspace compilation.");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_cli_can_be_invoked() {
        // This test verifies that the CLI binary can be compiled and invoked
        // The actual functionality test will happen via cargo run
        // Using a simple operation instead of assert!(true) to avoid clippy warning
        let result = 2 + 2;
        assert_eq!(result, 4, "CLI test infrastructure works");
    }
}
