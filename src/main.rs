//! `gumtree-rs` command-line tool.
//!
//! Usage:
//!     gumtree-rs <old> <new> [-f JSON|TEXT] [-l EXT]
//!
//! The language is auto-detected from the file extension unless `-l` is given.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use gumtree_rs::{diff_sources, format::to_json, languages, DiffOptions};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    let mut positional: Vec<&str> = Vec::new();
    let mut format = "TEXT".to_string();
    let mut lang_override: Option<String> = None;
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
    let profile = match languages::profile_for_ext(&ext) {
        Some(matched_profile) => matched_profile,
        None => {
            let filename = Path::new(old_path)
                .file_name()
                .and_then(|name_os_str| name_os_str.to_str())
                .unwrap_or("");
            match languages::profile_for_filename(filename) {
                Some(matched_profile) => matched_profile,
                None => {
                    eprintln!("unsupported language for extension: .{}", ext);
                    eprintln!("use -l EXT to override (e.g. -l rs, -l py)");
                    return ExitCode::from(2);
                }
            }
        }
    };

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

    let result = match diff_sources(&old_src, &new_src, profile, &DiffOptions::default()) {
        Ok(diff_result) => diff_result,
        Err(error) => {
            eprintln!("diff failed: {}", error);
            return ExitCode::from(1);
        }
    };

    if format.eq_ignore_ascii_case("JSON") {
        let json = to_json(
            &result.src_tree,
            &result.dst_tree,
            &result.mapping,
            &result.actions,
        );
        println!("{}", json);
    } else {
        for action in &result.actions {
            println!("{:?}", action);
        }
    }

    ExitCode::SUCCESS
}

fn print_usage(progname: &str) {
    eprintln!(
        "usage: {} <old-file> <new-file> [-f JSON|TEXT] [-l EXT]",
        progname
    );
    eprintln!();
    eprintln!("  -f FORMAT  output format: TEXT (default) or JSON");
    eprintln!("  -l EXT     override language (e.g. rs, py, js)");
    eprintln!("  -h         show this help");
}
