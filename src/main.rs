use anyhow::anyhow;
use clap;
use colored::Colorize;
use rayon::prelude::*;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use walkdir::WalkDir;

struct GitOutput {
    output: std::process::Output,
}

fn do_git_command(path: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
    match process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
    {
        Ok(output) => Ok(GitOutput { output }),
        Err(err) => Err(anyhow!(err)),
    }
}

#[derive(Debug)]
struct StandardOutputs {
    stdout: String,
    stderr: String,
}

impl StandardOutputs {
    fn from_utf8_lossy(stdout: &[u8], stderr: &[u8]) -> Self {
        return StandardOutputs {
            stdout: String::from_utf8_lossy(stdout).to_string(),
            stderr: String::from_utf8_lossy(stderr).to_string(),
        };
    }
}

#[derive(Debug)]
struct ProcessingResult<'a> {
    path: PathBuf,
    args: &'a [&'a str],
    kind: ProcessingResultKind,
}

#[derive(Debug)]
enum ProcessingResultKind {
    Failure(String),
    GitFailure(StandardOutputs),
    GitSuccess(StandardOutputs),
}

fn get_repositories_paths() -> anyhow::Result<Vec<PathBuf>> {
    let mut repositories_paths = Vec::<PathBuf>::new();

    for entry in WalkDir::new(".") {
        let entry = entry?;
        let entry_path = entry.into_path();

        let mut path = match entry_path.canonicalize() {
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => continue,
                _ => return Err(anyhow!(err)),
            },
            Ok(v) => v,
        };
        let path_string = path.to_string_lossy();

        if !path_string.ends_with(".git")
            || path_string.contains("third_party")
            || path_string.contains(".zigmod")
        {
            continue;
        }

        path.pop();

        repositories_paths.push(path);
    }

    Ok(repositories_paths)
}

fn main() {
    let matches = clap::Command::new("gitjuggling")
        .author("Vincent Rischmann <vincent@rischmann.fr>")
        .version("1.0")
        .about("Git juggler")
        .trailing_var_arg(true)
        .arg(clap::Arg::new("git_args").multiple_values(true))
        .get_matches();

    let git_args: Vec<&str> = matches
        .get_many::<String>("git_args")
        .unwrap()
        .map(|v| v.as_str())
        .collect();

    // Collect all local git repositories

    let repositories_paths = match get_repositories_paths() {
        Err(err) => panic!("unable to get repositories paths: {}", err),
        Ok(v) => v,
    };

    //

    let mut processing_results = Vec::<ProcessingResult>::new();

    let processing_iterator = repositories_paths.into_par_iter().map(|directory| {
        match do_git_command(&directory, &git_args) {
            Err(err) => ProcessingResult {
                path: directory,
                args: &git_args,
                kind: ProcessingResultKind::Failure(format!(
                    "unable to do git command, err: {}",
                    err
                )),
            },
            Ok(go) => {
                let standard_outputs =
                    StandardOutputs::from_utf8_lossy(&go.output.stdout, &go.output.stderr);

                if go.output.status.success() {
                    ProcessingResult {
                        path: directory,
                        args: &git_args,
                        kind: ProcessingResultKind::GitSuccess(standard_outputs),
                    }
                } else {
                    ProcessingResult {
                        path: directory,
                        args: &git_args,
                        kind: ProcessingResultKind::GitFailure(standard_outputs),
                    }
                }
            }
        }
    });
    processing_results.par_extend(processing_iterator);

    //

    let mut items_failed: usize = 0;
    let mut items_succeeded: usize = 0;

    for result in processing_results {
        println!(
            "{} executing {}",
            result.path.to_string_lossy().to_string().green(),
            result.args.join(" ").yellow()
        );

        match result.kind {
            ProcessingResultKind::Failure(err) => {
                items_failed += 1;
                println!("{}", &err);
            }
            ProcessingResultKind::GitFailure(outputs) => {
                items_failed += 1;
                io::stdout().write_all(outputs.stdout.as_bytes()).unwrap();
                io::stderr().write_all(outputs.stderr.as_bytes()).unwrap();
            }
            ProcessingResultKind::GitSuccess(outputs) => {
                items_succeeded += 1;
                io::stdout().write_all(outputs.stdout.as_bytes()).unwrap();
            }
        }
    }

    println!(
        "{} {} {} {}",
        format!("{}", items_succeeded).magenta(),
        "items succeeded,".blue(),
        format!("{}", items_failed).magenta(),
        "items failed".blue(),
    );

    if items_failed > 0 {
        process::exit(1);
    }
}
