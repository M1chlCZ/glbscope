use anyhow::{Context, Result};
use clap::Parser;
use glbscope::{Budget, Finding, Report, check, inspect};
use serde::Serialize;
use std::path::PathBuf;
use std::process::ExitCode;

/// Inspect GLB files and check them against a budget.
#[derive(Parser)]
#[command(name = "glbscope", version, about)]
struct Args {
    /// Budget file in TOML format.
    #[arg(long)]
    budget: Option<PathBuf>,
    /// Print one JSON document instead of a table.
    #[arg(long)]
    json: bool,
    /// Only print findings and the summary.
    #[arg(long)]
    quiet: bool,
    /// Files or directories to scan.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

#[derive(Serialize)]
struct FileResult {
    path: String,
    report: Option<Report>,
    error: Option<String>,
    findings: Vec<Finding>,
}

fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in walkdir::WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|entry| entry.ok())
            {
                let candidate = entry.path();
                if candidate.is_file()
                    && candidate
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
                {
                    files.push(candidate.to_path_buf());
                }
            }
        } else {
            files.push(path.clone());
        }
    }
    files.sort();
    files
}

fn run() -> Result<ExitCode> {
    let args = Args::parse();
    let budget = match &args.budget {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read budget file {}", path.display()))?;
            Some(Budget::from_toml(&text)?)
        }
        None => None,
    };

    let files = collect_files(&args.paths);
    let mut results = Vec::new();
    let mut total_findings = 0usize;
    let mut errors = 0usize;
    for file in &files {
        let path = file.display().to_string();
        match std::fs::read(file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| inspect(&bytes).map_err(|error| format!("{error:#}")))
        {
            Ok(report) => {
                let findings = match &budget {
                    Some(budget) => check(&report, budget),
                    None => Vec::new(),
                };
                total_findings += findings.len();
                if !args.json && !args.quiet {
                    println!(
                        "{path}  {} bytes  {} triangles  {} vertices  {} nodes  {} textures",
                        report.file_bytes,
                        report.triangles,
                        report.vertices,
                        report.nodes,
                        report.textures,
                    );
                }
                if !args.json {
                    for finding in &findings {
                        println!("  {} {} > {}", finding.rule, finding.actual, finding.limit);
                    }
                }
                results.push(FileResult {
                    path,
                    report: Some(report),
                    error: None,
                    findings,
                });
            }
            Err(message) => {
                errors += 1;
                eprintln!("{path}: {message}");
                results.push(FileResult {
                    path,
                    report: None,
                    error: Some(message),
                    findings: Vec::new(),
                });
            }
        }
    }

    if args.json {
        let document = serde_json::json!({
            "files": results,
            "findings": total_findings,
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&document)?);
    } else if !args.quiet {
        println!(
            "{} files, {} findings, {} errors",
            results.len(),
            total_findings,
            errors
        );
    }

    Ok(if errors > 0 {
        ExitCode::from(2)
    } else if total_findings > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("glbscope: {error:#}");
            ExitCode::from(2)
        }
    }
}
