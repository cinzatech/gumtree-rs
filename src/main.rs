//! `gumtree-rs` command-line tool.
//!
//! Usage:
//!     gumtree-rs textdiff <old> <new> [-f JSON|TEXT]
//!
//! Mimics the upstream `gumtree textdiff` CLI for YAML files.

use std::env;
use std::fs;
use std::process::ExitCode;

use gumtree_rs::{diff_sources, format::to_json, language::LanguageProfile, DiffOptions};

struct YamlProfile;
impl LanguageProfile for YamlProfile {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_yaml::LANGUAGE.into()
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        print_usage(&args.first().map(String::as_str).unwrap_or("gumtree-rs"));
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
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "-f" if i + 1 < args.len() => {
                format = args[i + 1].clone();
                i += 2;
            }
            other => {
                eprintln!("unexpected argument: {}", other);
                print_usage(&args[0]);
                return ExitCode::from(2);
            }
        }
    }

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

    let result = match diff_sources(&old_src, &new_src, &YamlProfile, &DiffOptions::default()) {
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
    eprintln!("usage: {} textdiff <old-file> <new-file> [-f JSON|TEXT]", progname);
}
