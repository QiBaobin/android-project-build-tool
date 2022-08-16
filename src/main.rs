mod api;
mod domain;
mod error;
mod util;

use dirs::home_dir;
use domain::*;
use error::Result;
use log::{trace, warn, LevelFilter};
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Opt {
    /// verbose, can be provided many times
    #[structopt(short, parse(from_occurrences))]
    verbose: usize,

    /// the regex of projects' name under root_project_dir we want to exclude always
    #[structopt(
        default_value = "module-templates|build-support|gradle|build-tools|root-project.*|buildSrc|^$",
        short,
        long
    )]
    excluded_projects: String,

    /// the project paths (outside of the root_project_dir) we want to include in this project when building
    /// if we need more than one path,  seperate them by space
    #[structopt(default_value = "", short, long, env = "INCLUDE_PROJECTS")]
    include_projects: String,

    /// if don't scan impacted projects
    #[structopt(long, env = "NO_SCAN_IMPACTED_PROJECTS")]
    not_scan_impacted_projects: bool,

    /// the gradel command to run for building, you can give args here too
    #[structopt(short, long, env = "GRADLE_CMD")]
    gradle_cmd: Option<String>,

    /// the command we wan to run
    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    cmd: Command,
}
#[derive(StructOpt, Debug)]
#[structopt(about = "the build tools commands")]
enum Command {
    /// build modules
    Build {
        /// if includes modules that don't change
        #[structopt(short, long)]
        all: bool,
        /// the compare commit to test if modules are changed, like head
        #[structopt(short = "c", long)]
        after_commit: Option<String>,
        /// the regex that project name shall match
        #[structopt(default_value = ".*", short, long)]
        projects: String,
        /// the file contains external triggers
        #[structopt(long, env = "TRIGGERS_FILE")]
        triggers_file: Option<String>,
        /// the task to run
        #[structopt(subcommand)]
        gradle: Option<GradleCommand>,
    },
    /// Control what modules will be included in default project
    Open {
        /// the regex that project name shall match
        #[structopt(default_value = ".*", short, long)]
        projects: String,
        /// if clean old IDE settings and build directory
        #[structopt(short, long)]
        clean: bool,
    },
    /// Create new features
    Create {
        /// the module name
        #[structopt(short, long)]
        path: String,
        /// the directory contains different type of templates
        #[structopt(default_value = "module-templates", short, long)]
        from: String,
        /// the list of template types seperated by comma(,) that the module want to contain , match the the dir name in the from dirtory
        #[structopt(default_value = "", short, long)]
        types: String,
        /// the list of files, directories seperated by comma(,) that would not copies from
        /// template modules
        #[structopt(default_value = ".DS_Store", long)]
        excludes: String,
    },
    /// Create a pull request
    PullRequest {
        /// the title of the PR
        #[structopt(short, long, default_value = "")]
        summary: String,
        /// the description
        #[structopt(short, long, default_value = "")]
        description: String,
        /// the branch name
        #[structopt(short, long, default_value = "")]
        branch_name: String,
        /// the to branch name
        #[structopt(short, long, default_value = "master")]
        to_branch: String,
        /// the user name of git stash
        #[structopt(short, long, env = "USER")]
        user: String,
        /// the password
        #[structopt(short, long, env = "GIT_PASSWORD", hide_env_values = true)]
        password: Option<String>,
        /// the reviewers
        #[structopt(short, long, env = "GIT_REVIEWERS")]
        reviewers: Vec<String>,
        /// the reviewer groups
        #[structopt(short = "R", long)]
        reviewer_group: Vec<PathBuf>,
        /// if open the pull request in the default web browser
        #[structopt(short, long)]
        open: bool,
        /// the file contains external triggers
        #[structopt(long, env = "TRIGGERS_FILE")]
        triggers_file: Option<String>,
    },
    Users {
        /// return only users, whose username, name or email address contain the filter value
        /// we support multiple filters by separating them with ;
        #[structopt(short, long)]
        query: String,
        /// the file names would write to
        #[structopt(short, long)]
        file: Option<PathBuf>,
        /// don't truncated the file, append the name there
        #[structopt(short, long)]
        append: bool,
        /// the user name of git stash
        #[structopt(short, long, env = "USER")]
        user: String,
        /// the password
        #[structopt(short, long, env = "GIT_PASSWORD", hide_env_values = true)]
        password: Option<String>,
    },
}
#[derive(StructOpt, Debug)]
#[structopt(about = "the build commands")]
enum GradleCommand {
    #[structopt(external_subcommand)]
    Other(Vec<String>),
}

fn main() -> Result<()> {
    openssl_probe::init_ssl_cert_env_vars();

    let opt = Opt::from_args();
    let _ = simple_logger::SimpleLogger::new()
        .with_level(match opt.verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        })
        .init();

    trace!("Arguments: {:?}", opt);

    let scan_impacted_projects = !opt.not_scan_impacted_projects;
    let mut ps = Projects::new(
        &opt.excluded_projects,
        &opt.include_projects,
        opt.gradle_cmd.clone(),
    );
    match opt.cmd {
        Command::Build {
            all,
            after_commit,
            projects,
            triggers_file,
            gradle,
        } => get_projects(
            ps,
            all,
            after_commit,
            &projects,
            scan_impacted_projects,
            triggers_file,
        )
        .build(
            &{
                match gradle {
                    Some(GradleCommand::Other(cmds)) => cmds,
                    None => vec!["build".to_string(), "publishModule".to_string()],
                }
            },
            opt.verbose,
        ),
        Command::Open { projects, clean } => {
            ps.scan(
                &ps.create_filters().with_name_regex(&projects),
                scan_impacted_projects,
            );
            ps.open(clean)
        }
        Command::Create {
            path,
            from,
            types,
            excludes,
        } => ps.create(&path, &from, &types, &excludes, scan_impacted_projects),
        Command::PullRequest {
            summary,
            description,
            branch_name,
            to_branch,
            user,
            password,
            reviewers,
            reviewer_group,
            open,
            triggers_file,
        } => {
            let mut all_reviewers = reviewers;
            let mut reviewer_group = reviewer_group;
            reviewer_group.push(default_reviewers_file());
            for g in reviewer_group {
                let path = g.to_string_lossy().to_string();
                if let Ok(mut file) = File::open(g) {
                    let mut contents = String::new();
                    file.read_to_string(&mut contents).map_or_else(
                        |e| warn!("Can't read file {:?} content caused by {:?}", &path, e),
                        |_| all_reviewers.push(contents),
                    );
                } else {
                    warn!("Can't open file {:?}", &path);
                }
            }
            trace!("Collect reviewers as:\n{:?}", &all_reviewers);
            ps.create_pull_request(
                Request {
                    title: summary,
                    description,
                    branch_name,
                    to_branch,
                    reviewers: all_reviewers,
                },
                Auth { user, password },
                opt.verbose,
                open,
                triggers_file,
            )
        }
        Command::Users {
            query,
            file,
            append,
            user,
            password,
        } => ps.query_users(
            query.split(';').collect(),
            Auth { user, password },
            &file.unwrap_or_else(default_reviewers_file),
            append,
        ),
    }
}

fn get_projects(
    mut ps: Projects,
    all: bool,
    after_commmit: Option<String>,
    name_pattern: &str,
    scan_impacted_projects: bool,
    triggers_file: Option<String>,
) -> Projects {
    let mut filters = ps.create_filters().with_name_regex(name_pattern);
    if !all {
        if let Some(commit) = after_commmit.or_else(|| {
            ps.vc()
                .remote_branch()
                .map_err(|e| {
                    warn!("Can't get remote branch {:?}", e);
                    e
                })
                .ok()
        }) {
            filters = filters.since_commit(ps.vc(), &commit, triggers_file.as_deref());
        }
    }
    ps.scan(&filters, scan_impacted_projects);
    ps
}

const GIT_REVIEWERS_FILE: &str = ".git_reviewers";
fn default_reviewers_file() -> PathBuf {
    let mut path = home_dir().unwrap();
    path.push(GIT_REVIEWERS_FILE);

    path
}
