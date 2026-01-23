use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use std::io::Write;

/// Format and print a success message
pub fn format_success(message: &str, use_colors: bool) {
    let mut stdout = if use_colors {
        StandardStream::stdout(ColorChoice::Always)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    };

    let mut color_spec = ColorSpec::new();
    color_spec.set_fg(Some(Color::Green));
    
    let _ = stdout.set_color(&color_spec);
    let _ = writeln!(&mut stdout, "{}", message);
    let _ = stdout.reset();
}

/// Format and print an error message  
pub fn format_error(message: &str, use_colors: bool) {
    let mut stderr = if use_colors {
        StandardStream::stderr(ColorChoice::Always)
    } else {
        StandardStream::stderr(ColorChoice::Never)
    };

    let mut color_spec = ColorSpec::new();
    color_spec.set_fg(Some(Color::Red));
    
    let _ = stderr.set_color(&color_spec);
    let _ = writeln!(&mut stderr, "Error: {}", message);
    let _ = stderr.reset();
}
