//! `gumtree-rs` command-line tool.
//!
//! Usage:
//!     gumtree-rs textdiff <old> <new> [-f JSON|TEXT] [-l LANG]
//!
//! The language is auto-detected from the file extension unless `-l` is given.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use gumtree_rs::{diff_sources, format::to_json, languages, DiffOptions};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        print_usage(args.first().map(String::as_str).unwrap_or("gumtree-rs"));
        return ExitCode::from(2);
    }

    let command = &args[1];
    if command != "textdiff" {
        eprintln!("unknown command: {}", command);
        print_usage(&args[0]);
        return ExitCode::from(2);
    }

    let old_path = &args[2];
    let new_path = &args[3];

    let mut format = "TEXT".to_string();
    let mut lang_override: Option<String> = None;
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "-f" if i + 1 < args.len() => {
                format = args[i + 1].clone();
                i += 2;
            }
            "-l" if i + 1 < args.len() => {
                lang_override = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("unexpected argument: {}", other);
                print_usage(&args[0]);
                return ExitCode::from(2);
            }
        }
    }

    // Determine the language extension to use.
    let ext = lang_override.unwrap_or_else(|| {
        Path::new(old_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string()
    });

    // Try extension first, then fall back to filename (for Dockerfile, Makefile, etc.).
    let profile = match languages::profile_for_ext(&ext) {
        Some(p) => p,
        None => {
            let filename = Path::new(old_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            match languages::profile_for_filename(filename) {
                Some(p) => p,
                None => {
                    eprintln!(
                        "unsupported file extension: .{}\nsupported: {}",
                        ext,
                        languages::supported_extensions().join(", ")
                    );
                    return ExitCode::from(2);
                }
            }
        }
    };

    let old_src = match fs::read(old_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read {}: {}", old_path, e);
            return ExitCode::from(1);
        }
    };
    let new_src = match fs::read(new_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read {}: {}", new_path, e);
            return ExitCode::from(1);
        }
    };

    let result = match diff_sources(&old_src, &new_src, profile, &DiffOptions::default()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("diff failed: {}", e);
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
        "usage: {} textdiff <old-file> <new-file> [-f JSON|TEXT] [-l EXT]",
        progname
    );
    eprintln!("  -l EXT   override language (e.g. rs, py, js)");
    eprintln!(
        "  supported extensions: {}",
        languages::supported_extensions().join(", ")
    );
}
