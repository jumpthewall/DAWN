//! Test report generation and display.
//!
//! Provides formatted console output for censorship test results.

use super::resolver::TestResults;
use colored::Colorize;

/// Prints the full test report comparing all strategies.
pub fn print_report(system_results: &TestResults, plugin_results: &[(&str, TestResults)]) {
    println!();
    println!("{}", "═".repeat(60).bold());
    println!(
        "{}",
        "                    DAWN Censorship Test Report"
            .bold()
            .cyan()
    );
    println!("{}", "═".repeat(60).bold());
    println!();

    // System resolver results
    print_strategy_results("System Resolver (Baseline)", system_results);

    // Plugin results
    for (name, results) in plugin_results {
        print_strategy_results(&format!("Plugin: {}", name), results);
    }

    // Summary comparison
    if !plugin_results.is_empty() {
        print_summary(system_results, plugin_results);
    }
}

/// Print results for a single strategy
fn print_strategy_results(name: &str, results: &TestResults) {
    println!("{}", "─".repeat(60));
    println!("{}", name.bold().yellow());
    println!("{}", "─".repeat(60));

    println!(
        "  Total domains tested: {}",
        results.total.to_string().bold()
    );

    let censored_pct = format!("{:.2}%", results.censored_percentage());
    let censored_str = format!("  Censored: {} ({})", results.censored, censored_pct);
    println!("{}", censored_str.red());

    let not_censored_pct = format!("{:.2}%", results.not_censored_percentage());
    let not_censored_str = format!(
        "  Not censored: {} ({})",
        results.not_censored, not_censored_pct
    );
    println!("{}", not_censored_str.green());

    println!("  Timeouts: {}", results.timeouts.to_string().yellow());
    println!("  Errors: {}", results.errors.to_string().yellow());
    println!();
}

/// Print summary comparison between strategies
fn print_summary(system_results: &TestResults, plugin_results: &[(&str, TestResults)]) {
    println!("{}", "─".repeat(60));
    println!("{}", "Summary Comparison".bold().cyan());
    println!("{}", "─".repeat(60));

    let system_censored = system_results.censored_percentage();
    println!("  {}: {:.2}% censored", "System".bold(), system_censored);

    for (name, results) in plugin_results {
        let plugin_censored = results.censored_percentage();
        let improvement = system_censored - plugin_censored;

        let improvement_str = if improvement > 0.0 {
            format!("(+{:.2}% improvement)", improvement)
                .green()
                .to_string()
        } else if improvement < 0.0 {
            format!("({:.2}% worse)", improvement.abs())
                .red()
                .to_string()
        } else {
            "(no change)".to_string()
        };

        println!(
            "  {}: {:.2}% censored {}",
            name.bold(),
            plugin_censored,
            improvement_str
        );
    }
    println!();
}

/// Print progress update during testing
pub fn print_progress(strategy: &str, completed: usize, total: usize) {
    let pct = (completed as f64 / total as f64) * 100.0;
    print!(
        "\r  {} [{}/{}] {:.1}%",
        strategy.cyan(),
        completed,
        total,
        pct
    );
    use std::io::{self, Write};
    let _ = io::stdout().flush();
}

/// Clear the progress line
pub fn clear_progress() {
    print!("\r{}\r", " ".repeat(60));
    use std::io::{self, Write};
    let _ = io::stdout().flush();
}
