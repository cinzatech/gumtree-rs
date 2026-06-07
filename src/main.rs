//! `gumtree-rs` command-line tool.
//!
//! Usage:
//!     gumtree-rs <old> <new> [-f JSON|TEXT|SIDE] [-l EXT]
//!
//! The language is auto-detected from the file extension unless `-l` is given.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use gumtree_rs::output::json::JsonFormatter;
use gumtree_rs::output::terminal::TerminalFormatter;
use gumtree_rs::output::text::TextFormatter;
use gumtree_rs::output::{DiffFormatter, FormatInput};
use gumtree_rs::{diff_lines, diff_sources, languages, DiffOptions};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    let mut positional: Vec<&str> = Vec::new();
    let mut format = "TEXT".to_string();
    let mut lang_override: Option<String> = None;
    let mut max_file_size: Option<u64> = None;
    let mut parse_timeout: Option<u64> = None;
    let mut arg_index = 1;
    while arg_index < args.len() {
        match args[arg_index].as_str() {
            "-h" | "--help" => {
                print_usage(args.first().map(String::as_str).unwrap_or("gumtree-rs"));
                return ExitCode::SUCCESS;
            }
            "-f" if arg_index + 1 < args.len() => {
                format = args[arg_index + 1].clone();
                arg_index += 2;
            }
            "-l" if arg_index + 1 < args.len() => {
                lang_override = Some(args[arg_index + 1].clone());
                arg_index += 2;
            }
            "--max-file-size" if arg_index + 1 < args.len() => {
                let Ok(value) = args[arg_index + 1].parse::<u64>() else {
                    eprintln!("invalid value for --max-file-size: {}", args[arg_index + 1]);
                    return ExitCode::from(2);
                };
                max_file_size = Some(value);
                arg_index += 2;
            }
            "--parse-timeout" if arg_index + 1 < args.len() => {
                let Ok(value) = args[arg_index + 1].parse::<u64>() else {
                    eprintln!("invalid value for --parse-timeout: {}", args[arg_index + 1]);
                    return ExitCode::from(2);
                };
                parse_timeout = Some(value);
                arg_index += 2;
            }
            unknown if unknown.starts_with('-') => {
                eprintln!("unknown option: {}", unknown);
                print_usage(&args[0]);
                return ExitCode::from(2);
            }
            _ => {
                positional.push(&args[arg_index]);
                arg_index += 1;
            }
        }
    }

    if positional.len() != 2 {
        print_usage(args.first().map(String::as_str).unwrap_or("gumtree-rs"));
        return ExitCode::from(2);
    }
    let old_path = positional[0];
    let new_path = positional[1];

    // Determine the language extension to use.
    let ext = lang_override.unwrap_or_else(|| {
        Path::new(old_path)
            .extension()
            .and_then(|ext_os_str| ext_os_str.to_str())
            .unwrap_or("")
            .to_string()
    });

    // Try extension first, then fall back to filename (for Dockerfile, Makefile, etc.).
    let profile = languages::profile_for_ext(&ext).or_else(|| {
        let filename = Path::new(old_path)
            .file_name()
            .and_then(|name_os_str| name_os_str.to_str())
            .unwrap_or("");
        languages::profile_for_filename(filename)
    });

    let old_src = match fs::read(old_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("failed to read {}: {}", old_path, error);
            return ExitCode::from(1);
        }
    };
    let new_src = match fs::read(new_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("failed to read {}: {}", new_path, error);
            return ExitCode::from(1);
        }
    };

    let mut diff_options = DiffOptions::default();
    if let Some(size) = max_file_size {
        diff_options.max_file_size = size;
    }
    if let Some(timeout_secs) = parse_timeout {
        diff_options.parse_timeout_us = timeout_secs.saturating_mul(1_000_000);
    }

    let diff_result = match profile {
        Some(lang_profile) => diff_sources(&old_src, &new_src, lang_profile, &diff_options),
        None => diff_lines(&old_src, &new_src, &diff_options),
    };
    let result = match diff_result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("diff failed: {}", error);
            return ExitCode::from(1);
        }
    };

    let input = FormatInput {
        source_bytes: &old_src,
        destination_bytes: &new_src,
        result: &result,
    };

    if format.eq_ignore_ascii_case("JSON") {
        let output = JsonFormatter::format(&input);
        println!("{}", output);
    } else if format.eq_ignore_ascii_case("SIDE") {
        let output = TerminalFormatter::format(&input);
        print!("{}", output);
    } else {
        let output = TextFormatter::format(&input);
        print!("{}", output);
    }

    ExitCode::SUCCESS
}

fn print_usage(progname: &str) {
    eprintln!(
        "usage: {} <old-file> <new-file> [-f JSON|TEXT|SIDE] [-l EXT]",
        progname
    );
    eprintln!();
    eprintln!("  -f FORMAT          output format: TEXT (default), JSON, or SIDE");
    eprintln!("  -l EXT             override language (e.g. rs, py, js)");
    eprintln!(
        "  --max-file-size N  max input file size in bytes (default: 104857600, 0 = no limit)"
    );
    eprintln!("  --parse-timeout N  parser timeout in seconds (default: 60, 0 = no limit)");
    eprintln!("  -h                 show this help");
}
