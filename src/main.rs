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

fn main() {
    let matches = clap::Command::new("gitjuggling")
        .author("Vincent Rischmann <vincent@rischmann.fr>")
        .version("1.0")
        .about("Git juggler")
        .trailing_var_arg(true)
        .arg(clap::Arg::new("depth").long("depth").short('d').num_args(1))
        .arg(clap::Arg::new("git_args").num_args(1..))
        .get_matches();

    let git_args: Vec<&str> = matches
        .get_many::<String>("git_args")
        .unwrap_or_default()
        .map(String::as_str)
        .collect();

    // Collect all local git repositories

    let depth = matches.get_one::<usize>("depth").copied().unwrap_or(3);

    let repositories_paths = match get_repositories_paths(depth) {
        Err(err) => panic!("unable to get repositories paths: {}", err),
        Ok(v) => v,
    };

    //

    let items_failed = Arc::<AtomicUsize>::new(AtomicUsize::new(0));
    let items_succeeded = Arc::<AtomicUsize>::new(AtomicUsize::new(0));

    repositories_paths.into_par_iter().for_each(|path| {
        let mut stdout = String::new();
        let mut stderr = String::new();

        writeln!(
            &mut stdout,
            "{} executing {}",
            &path.to_string_lossy().to_string().green(),
            &git_args.join(" ").yellow()
        )
        .unwrap();

        match do_git_command(&path, &git_args) {
            Err(err) => {
                items_failed.fetch_add(1, Ordering::SeqCst);
                write!(&mut stderr, "unable to do git command, err: {}", err).unwrap();
            }
            Ok(go) => {
                let go_stdout = String::from_utf8_lossy(&go.output.stdout);
                let go_stderr = String::from_utf8_lossy(&go.output.stderr);

                if go.output.status.success() {
                    items_succeeded.fetch_add(1, Ordering::SeqCst);
                    write!(&mut stdout, "{}", go_stdout).unwrap();
                } else {
                    items_failed.fetch_add(1, Ordering::SeqCst);
                    write!(&mut stdout, "{}", go_stdout,).unwrap();
                    write!(&mut stderr, "{}", go_stderr,).unwrap();
                }
            }
        }

        io::stdout().write_all(stdout.as_bytes()).unwrap();
        io::stderr().write_all(stderr.as_bytes()).unwrap();
    });

    //

    let succeeded = items_succeeded.load(Ordering::SeqCst);
    let failed = items_failed.load(Ordering::SeqCst);

    println!(
        "{} {} {} {}",
        format!("{}", succeeded).magenta(),
        "items succeeded,".blue(),
        format!("{}", failed).magenta(),
        "items failed".blue(),
    );

    if failed > 0 {
        process::exit(1);
    }
}
