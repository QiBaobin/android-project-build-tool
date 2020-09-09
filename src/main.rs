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

    /// the dirtory to contain root build.gradle.kts, settings.gradle.kts etc.
    #[structopt(default_value = "root-project", short, long)]
    root_project_dir: String,

    /// the regex of projects' name under root_project_dir we want to exclude always
    #[structopt(
        default_value = "module-templates|build-tools|root-project.*",
        short,
        long
    )]
    excluded_projects: String,

    /// the gradel command to run for building, you can give args here too
    #[structopt(short, long, env = "GRADLE_CMD")]
    gradle_cmd: Option<String>,

    /// if projects contains local references, if so we can't build module separately
    #[structopt(short, long)]
    contain_local_references: bool,

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
        /// the task to run
        #[structopt(short, long)]
        tasks: Vec<String>,
        /// the regex that project name shall match
        #[structopt(default_value = ".*", short, long)]
        projects: String,
        /// how many projects shall run in one gradle process
        #[structopt(default_value = "4", short, long)]
        number_of_projects_run_together: usize,
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
        #[structopt(default_value = "root-project/module-templates", short, long)]
        from: String,
        /// the list of template types seperated by comma(,) that the module want to contain , match the the dir name in the from dirtory
        #[structopt(default_value = "", short, long)]
        types: String,
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

    let mut ps = Projects::new(
        &opt.root_project_dir,
        &opt.excluded_projects,
        opt.gradle_cmd.clone(),
    );
    match opt.cmd {
        Command::Build {
            all,
            after_commit,
            mut tasks,
            projects,
            number_of_projects_run_together,
        } => get_projects(ps, all, after_commit, &projects).build(
            &{
                if tasks.is_empty() {
                    tasks.push("build".to_string());
                    tasks.push("publishModule".to_string());
                }
                tasks
            },
            number_of_projects_run_together,
            opt.verbose,
        ),
        Command::Open { projects, clean } => {
            ps.scan(&ps.create_filters().with_name_regex(&projects));
            ps.open(clean)
        }
        Command::Create { path, from, types } => ps.create(&path, &from, &types),
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
                opt.contain_local_references,
                open,
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
            filters = filters.since_commit(ps.vc(), &commit);
        }
    }
    ps.scan(&filters);
    ps
}

const GIT_REVIEWERS_FILE: &str = ".git_reviewers";
fn default_reviewers_file() -> PathBuf {
    let mut path = home_dir().unwrap();
    path.push(GIT_REVIEWERS_FILE);

    path
}
