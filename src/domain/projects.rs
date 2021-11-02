use super::*;
use crate::util::vc::GitVersionControl;
use glob::glob;
use regex::Regex;
use std::fs::{read_to_string, File, OpenOptions};
use std::io::prelude::*;
use std::ops::Deref;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
}
pub struct Projects {
    root_project_dir: String,
    excluded_projects: String,
    gradle_cmd: Option<String>,
    projects: Vec<Project>,
    vc: GitVersionControl,
}

impl Deref for Projects {
    type Target = Vec<Project>;

    fn deref(&self) -> &Vec<Project> {
        &self.projects
    }
}

impl Projects {
    pub fn new(excluded_projects: &str, gradle_cmd: Option<String>) -> Self {
        let vc = GitVersionControl::new();
        let root = vc.root();
        let root_project_dir =
            if root.join("settings.gradle").exists() || root.join("settings.gradle.kts").exists() {
                root.to_str().unwrap().to_string()
            } else {
                match glob(root.join("*").join("settings.gradle*").to_str().unwrap()) {
                    Ok(mut files) => files.next().map_or_else(
                        || ".".to_string(),
                        |f| f.unwrap().parent().unwrap().to_str().unwrap().to_string(),
                    ),
                    _ => " ".to_string(),
                }
            };

        Self {
            root_project_dir,
            excluded_projects: excluded_projects.to_string(),
            gradle_cmd,
            projects: vec![],
            vc,
        }
    }

    pub fn gradle_cmd(&self) -> String {
        self.gradle_cmd.as_ref().map_or_else(
            || {
                self.root_project()
                    .join("gradlew")
                    .to_str()
                    .unwrap()
                    .to_string()
            },
            |c| c.clone(),
        )
    }

    pub fn create_filters(&self) -> ProjectFilters {
        ProjectFilters(vec![]).exclude_projects(&self.excluded_projects)
    }

    pub fn scan(&mut self, filters: &ProjectFilters) {
        let root = &self.vc.root();
        let path = root.join("**").join("build.gradle*");
        let build_files = path.to_str().unwrap();
        let filters = &filters.0;
        match glob(build_files) {
            Ok(files) => {
                self.projects = files
                    .filter_map(|f| {
                        f.map_or_else(
                            |e| {
                                warn!("Io error {:?}", e);
                                None
                            },
                            Some,
                        )
                    })
                    .map(|f| {
                        let project_dir = f.parent().unwrap();
                        Project {
                            path: project_dir.to_path_buf(),
                            name: project_dir
                                .strip_prefix(root)
                                .unwrap()
                                .iter()
                                .map(|os_str| os_str.to_str().unwrap())
                                .collect::<Vec<_>>()
                                .join("-"),
                        }
                    })
                    .filter(|p| {
                        let required = filters.iter().all(|f| f(p));
                        if required {
                            info!("Found project met criteria: {}", p.name);
                        } else {
                            debug!(
                                "Found project {} that doesn't Meet criteria, no need to run",
                                p.name
                            );
                        }
                        required
                    })
                    .collect()
            }
            Err(e) => panic!("Can't detect projects caused by {:?}", e),
        };
    }

    pub fn vc(&self) -> &dyn VersionControl {
        &self.vc
    }

    pub fn root(&self) -> PathBuf {
        self.vc.root()
    }

    pub fn root_project(&self) -> PathBuf {
        self.vc().root().join(&self.root_project_dir)
    }

    pub fn create_settings(&self, file: &Path) -> Result<()> {
        self.add_to_settings_file(self.iter(), file)
    }

    pub fn create_settings_for_subprojects<'projects, I: Iterator<Item = &'projects Project>>(
        &'projects self,
        subprojects: I,
        file: &Path,
    ) -> Result<()> {
        self.add_to_settings_file(subprojects, file)
    }

    pub fn add_to_default_settings_file(&self) -> Result<()> {
        let mut file = self.root_project();
        file.push("settings.gradle.kts");
        self.create_settings(&file)
    }

    pub fn append_to_default_settings_file(&self) -> Result<PathBuf> {
        let file = self.root_project().join("settings.gradle.kts");
        if file.exists() {
            let mut f = OpenOptions::new()
                .append(true)
                .open(&file)
                .map_err(|e| Error::new(&format!("Can't open {:?}", &file), e))?;
            self.writ_to_settings_file(self.iter(), &mut f)?;
            Ok(file)
        } else {
            Err(Error::from_str(&format!(
                "There is no settings file exists at {:?}",
                &file
            )))
        }
    }

    fn add_to_settings_file<'projects, I: Iterator<Item = &'projects Project>>(
        &'projects self,
        projects: I,
        file: &Path,
    ) -> Result<()> {
        info!("Creating settings file: {:?}", file);

        let mut file = File::create(file).map_err(|e| Error::new("Can't create the file", e))?;
        let path = self.root_project().join("settings.pre.gradle.kts");
        if path.exists() {
            read_to_string(path)
                .and_then(|s| write!(file, "{}", s))
                .err()
                .map(|e| {
                    warn!(
                        "Can't add the content of settings.pre.gradle.kts caused by {:?}",
                        e
                    );
                    e
                });
        }
        self.writ_to_settings_file(projects, &mut file)
    }

    fn writ_to_settings_file<'projects, I: Iterator<Item = &'projects Project>>(
        &'projects self,
        mut projects: I,
        file: &mut File,
    ) -> Result<()> {
        fn relative_to(p: &Path, from_dir: &Path) -> PathBuf {
            let mut from = from_dir.to_path_buf();
            let mut pb = PathBuf::from(".");
            while !p.starts_with(&from) {
                pb.push("..");
                from.pop();
            }
            pb.push(p.strip_prefix(&from).unwrap());
            pb
        }

        projects
            .try_for_each(|p| {
                write!(
                    file,
                    "include(\":{}\")\nproject(\":{}\").projectDir = file(\"{}\")\n\n",
                    &p.name,
                    &p.name,
                    relative_to(&p.path.as_path(), &self.root_project()).display()
                )
            })
            .map_err(|e| Error::new(&format!("Can't write to {:?}", &file), e))
    }
}

pub struct ProjectFilters(Vec<Box<dyn Fn(&Project) -> bool>>);
impl ProjectFilters {
    pub fn with_name_regex(mut self, pattern: &str) -> Self {
        match Regex::new(pattern) {
            Ok(regex) => {
                self.0.push(Box::new(move |p| regex.is_match(&p.name)));
            }
            Err(e) => {
                warn!("Bad regex for project name: {}\n{:?}", pattern, e);
            }
        };
        self
    }

    pub fn exclude_projects(mut self, pattern: &str) -> Self {
        match Regex::new(pattern) {
            Ok(regex) => {
                self.0.push(Box::new(move |p| !regex.is_match(&p.name)));
            }
            Err(e) => {
                warn!("Bad regex for project name: {}\n{:?}", pattern, e);
            }
        };
        self
    }

    pub fn since_commit(mut self, vc: &dyn VersionControl, hash: &str) -> Self {
        let root = vc.root();
        match vc.diff_files(hash) {
            Ok(files) => self.0.push(Box::new(move |p| {
                files.iter().any(|f| {
                    let relative = if p.path.is_absolute() {
                        p.path.strip_prefix(&root).unwrap_or(&p.path)
                    } else {
                        &p.path
                    };

                    Path::new(f).strip_prefix(relative).is_ok()
                        || p.name.starts_with(f.trim_end_matches('/'))
                })
            })),
            Err(e) => {
                warn!(
                    "Can't get diff files with commit {} caused by {:?}",
                    hash, e
                );
            }
        };
        self
    }
}

pub trait VersionControl {
    fn root(&self) -> std::path::PathBuf;

    fn remote_branch(&self) -> Result<String>;

    fn diff_files(&self, commit: &str) -> Result<Vec<String>>;

    fn add_path(&self, path: &std::path::Path) -> Result<()>;

    fn commit(&self, message: &str) -> Result<()>;

    fn get_push_url(&self) -> Result<String>;

    fn log(&self, range: &str, include_body: bool) -> Result<Vec<String>>;

    fn fetch(&self, branch: &str) -> Result<()>;

    fn push(&self, branch: &str) -> Result<()>;

    fn merge(&self, remote_branch: &str, dry_run: bool) -> Result<()>;
}
