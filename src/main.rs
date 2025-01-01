#![allow(clippy::uninlined_format_args)]

use anyhow::anyhow;
use colored::Colorize;
use gitmodules::GitModules;
use rayon::prelude::*;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

mod gitmodules;

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

fn parse_gitmodules(path: &Path) -> anyhow::Result<GitModules> {
    let contents = {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        contents
    };

    let gitmodules = GitModules::parse(&contents)?;

    Ok(gitmodules)
}

fn is_submodule(path: &Path, gitmodules: Option<&GitModules>) -> bool {
    match gitmodules {
        Some(gitmodules) => {
            // If this is a submodule:
            // * path is the git submodule directory
            // * parent path is the parent git repository containing the gitmodules

            let parent_path = match path.parent().ok_or(anyhow!("no parent path")) {
                Ok(path) => path,
                Err(_) => return false,
            };

            let tmp = parent_path
                .components()
                .last()
                .map(|p| PathBuf::from(p.as_os_str()))
                .unwrap_or_default();

            gitmodules.contains(&tmp)
        }
        None => false,
    }
}

fn get_repositories_paths(depth: usize) -> anyhow::Result<Vec<PathBuf>> {
    let mut repositories_paths = Vec::<PathBuf>::new();

    let walker = WalkDir::new(".").max_depth(depth);

    let mut gitmodules: Option<GitModules> = None;

    for entry in walker {
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

        // Parse the gitmodules file if it exists
        let gitmodules_path = path.join(".gitmodules");
        if gitmodules_path.exists() {
            if let Ok(tmp) = parse_gitmodules(&gitmodules_path) {
                gitmodules = Some(tmp)
            }
        }

        // Ignore directories that aren't a git repository
        if !path_string.ends_with(".git") {
            continue;
        }
        // Ignore repositories that are a submoduile
        if is_submodule(&path, gitmodules.as_ref()) {
            continue;
        }

        path.pop();

        repositories_paths.push(path);
    }

    Ok(repositories_paths)
}

struct Item {
    path: PathBuf,
    success: bool,
    stdout: String,
    stderr: String,
    err: Option<anyhow::Error>,
}

fn main() {
    let matches = clap::Command::new("gitjuggling")
        .disable_version_flag(true)
        .about("Git juggler")
        .arg(clap::Arg::new("depth").long("depth").short('d').num_args(1))
        .arg(
            clap::Arg::new("git_args")
                .num_args(1..)
                .required(true)
                .trailing_var_arg(true),
        )
        .get_matches();

    let git_args: Vec<&str> = matches
        .get_many::<String>("git_args")
        .unwrap_or_default()
        .map(String::as_str)
        .collect();

    // Setup rayon.

    // Can't use to many threads due to SSH multiplexing
    rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build_global()
        .unwrap();

    // Collect all local git repositories

    let depth = matches.get_one::<usize>("depth").copied().unwrap_or(3);

    let repositories_paths = match get_repositories_paths(depth) {
        Err(err) => panic!("unable to get repositories paths: {}", err),
        Ok(v) => v,
    };

    //

    let results: Vec<Item> = repositories_paths
        .into_par_iter()
        .map(|path| {
            let mut stdout = String::new();

            match do_git_command(&path, &git_args) {
                Err(err) => Item {
                    path: path.clone(),
                    success: false,
                    stdout,
                    stderr: String::new(),
                    err: Some(err),
                },
                Ok(go) => {
                    let go_stdout = String::from_utf8_lossy(&go.output.stdout);
                    stdout.push_str(&go_stdout);

                    let stderr = String::from_utf8_lossy(&go.output.stderr).to_string();

                    Item {
                        path: path.clone(),
                        success: go.output.status.success(),
                        stdout,
                        stderr,
                        err: None,
                    }
                }
            }
        })
        .collect();

    let (succeeded, failed): (Vec<_>, Vec<_>) = results
        .into_iter()
        .filter(|item| item.success)
        .partition(|item| item.success);

    //

    for item in &succeeded {
        println!(
            "{} executing {}",
            &item.path.to_string_lossy().to_string().green(),
            &git_args.join(" ").yellow()
        );

        print!("{}", item.stdout);

        if let Some(err) = &item.err {
            println!("error: {}", err);
        } else {
            print!("{}", item.stderr);
        }
    }

    println!(
        "\n{}{}{}\n",
        "=== ".bright_white(),
        "Summary".bright_cyan(),
        " ===".bright_white()
    );

    println!(
        "{} {}",
        "Succeeded: ".blue(),
        format!("{}", succeeded.len()).bright_green()
    );
    println!(
        "{} {}",
        "Failed:    ".blue(),
        format!("{}", failed.len()).bright_red()
    );

    if !failed.is_empty() {
        process::exit(1);
    }
}
