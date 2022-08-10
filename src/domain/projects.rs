use super::*;
use crate::util::vc::GitVersionControl;
use globwalk;
use rayon::prelude::*;
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
    excluded_projects: String,
    include_projects: String,
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
    pub fn new(
        excluded_projects: &str,
        include_projects: &str,
        gradle_cmd: Option<String>,
    ) -> Self {
        Self {
            excluded_projects: excluded_projects.to_string(),
            include_projects: include_projects.to_string(),
            gradle_cmd,
            projects: vec![],
            vc: GitVersionControl::new(),
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

    pub fn scan(&mut self, filters: &ProjectFilters, scan_impacted_projects: bool) {
        let root = &self.vc.root();

        self.projects = self.scan_from(root, filters, scan_impacted_projects);
        for alt in self.include_projects.split_whitespace() {
            let mut p = root.clone();
            p.push(alt);
            self.projects
                .append(&mut self.scan_from(&p, filters, scan_impacted_projects));
        }
    }

    pub fn vc(&self) -> &dyn VersionControl {
        &self.vc
    }

    pub fn root(&self) -> PathBuf {
        self.vc.root()
    }

    pub fn root_project(&self) -> PathBuf {
        self.vc().root()
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
            self.write_to_settings_file(self.iter(), &mut f)?;
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
        let _ = write!(file, "// this is auto generated, please don't edit.\n// You can add logic in settings.pre.gradle.kts instead.\n// Ue `abt -v open` can regenerate this file.\n");
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
        self.write_to_settings_file(projects, &mut file)
    }

    fn write_to_settings_file<'projects, I: Iterator<Item = &'projects Project>>(
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
                    relative_to(p.path.as_path(), &self.root_project()).display()
                )
            })
            .map_err(|e| Error::new(&format!("Can't write to {:?}", &file), e))
    }

    fn scan_from(
        &self,
        from: &Path,
        filters: &ProjectFilters,
        scan_impacted_projects: bool,
    ) -> Vec<Project> {
        let filters = &filters.0;
        let files =
            globwalk::GlobWalkerBuilder::from_patterns(from, &["build.gradle", "build.gradle.kts"])
                .max_depth(1)
                .max_depth(3)
                .follow_links(false)
                .build()
                .unwrap()
                .into_iter()
                .filter_map(std::result::Result::ok);

        let mut matched = vec![];
        let mut others = vec![];
        for f in files {
            let f = f.path();
            let project_dir = f.parent().unwrap();
            let p = Project {
                path: project_dir.to_path_buf(),
                name: project_dir
                    .strip_prefix(from)
                    .unwrap()
                    .iter()
                    .map(|os_str| os_str.to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(":")
                    .replace(":android", "-android")
                    .replace(":domain", "-domain"),
            };

            if filters.iter().all(|f| f(&p)) {
                info!("Found project met criteria: {}", p.name);
                matched.push((p, f.to_path_buf(), None));
            } else {
                others.push((p, f.to_path_buf(), None));
            }
        }
        if scan_impacted_projects && !others.is_empty() && !matched.is_empty() {
            read_dependencies(&mut others);

            let exclude_rule = self.create_filters().0;
            let mut searched = 0;
            while searched < matched.len() {
                let end = matched.len();

                let mut i = 0;
                while i < others.len() {
                    let m = &others[i];
                    if m.2.as_ref().map_or(false, |v| {
                        v.iter()
                            .any(|p| matched[searched..end].iter().any(|(r, _, _)| r.name == *p))
                    }) {
                        let name = &m.0.name;
                        if exclude_rule.iter().all(|r| r(&m.0)) {
                            info!("Project {} is impacted, added too", name);
                            matched.push(others.swap_remove(i));
                        } else {
                            info!("Project {} is impacted, but it's excluded", name);
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
                searched = end;
            }
        }
        if !others.is_empty() {
            let mut searched = 0;
            while searched < matched.len() {
                read_dependencies(&mut matched[searched..]);
                let mut deps = vec![];
                for (_, _, v) in matched[searched..].iter() {
                    for d in v.as_ref().unwrap() {
                        deps.push(d);
                    }
                }
                searched = matched.len();

                deps.dedup();
                let mut i = 0;
                let mut added = vec![];
                while i < others.len() {
                    if deps.iter().any(|d| **d == others[i].0.name) {
                        info!("Project dependency: {}", others[i].0.name);
                        added.push(others.swap_remove(i));
                    } else {
                        i += 1;
                    }
                }
                matched.append(&mut added);
            }
        }

        matched.into_iter().map(|(p, _, _)| p).collect()
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
            Ok(files) => {
                let mut triggers = vec![];
                if let Ok(file) = read_to_string(root.join("build.triggers")) {
                    for l in file.lines() {
                        let fields: Vec<_> = l.split(':').collect();
                        if fields.len() == 2 {
                            if let Ok(regex) = Regex::new(fields[0]) {
                                triggers.push((
                                    regex,
                                    fields[1]
                                        .split(',')
                                        .map(|s| s.to_string())
                                        .collect::<Vec<_>>(),
                                ));
                            }
                        }
                    }
                }
                self.0.push(Box::new(move |p| {
                    let relative = if p.path.is_absolute() {
                        p.path.strip_prefix(&root).unwrap_or(&p.path)
                    } else {
                        &p.path
                    };
                    let triggers: Vec<_> = triggers
                        .iter()
                        .filter_map(|(regex, v)| {
                            relative.to_str().and_then(|p| {
                                if regex.is_match(p) {
                                    Some(v)
                                } else {
                                    None
                                }
                            })
                        })
                        .flatten()
                        .collect();
                    files.iter().any(|f| {
                        Path::new(f).strip_prefix(relative).is_ok()
                            || p.name.starts_with(f.trim_end_matches('/'))
                            || triggers
                                .iter()
                                .any(|fp| fp.starts_with(f.trim_end_matches('/')))
                    })
                }))
            }
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

fn read_dependencies(ps: &mut [(Project, PathBuf, Option<Vec<String>>)]) {
    let dep_regex = Regex::new(r#"['"]:([^'"]+)"#).unwrap();
    ps.as_mut().into_par_iter().for_each(|mut i| {
        if i.2.is_none() {
            let mut dependences = vec![];
            if let Ok(content) = read_to_string(&i.1) {
                for l in content.lines() {
                    if l.trim_start().starts_with("//") || !l.contains("project(") {
                        continue;
                    }
                    if let Some(cap) = dep_regex.captures(l) {
                        dependences.push(cap[1].into());
                    }
                }
            }
            i.2 = Some(dependences);
        }
    });
}
