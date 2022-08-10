use super::*;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

impl Projects {
    pub fn create(
        &mut self,
        path: &str,
        from: &str,
        types: &str,
        excludes: &str,
        scan_impacted_projects: bool,
    ) -> Result<()> {
        let template_path = self.relative_to_root(from);
        let module_types: Vec<_> = if types.trim().is_empty() {
            fs::read_dir(&template_path)
                .unwrap()
                .filter_map(|de| de.unwrap().file_name().to_str().map(|s| s.to_string()))
                .collect()
        } else {
            types
                .split(',')
                .filter_map(|s| {
                    let name = s.trim();
                    if name.is_empty() {
                        None
                    } else if !template_path.join(name).is_dir() {
                        warn!(
                            "There is no such type: {} in the template dir {:?}",
                            name, template_path
                        );
                        None
                    } else {
                        Some(name.to_string())
                    }
                })
                .collect()
        };

        if module_types.is_empty() {
            return Err(Error::from_str(&format!(
                "There is no module template in {}",
                template_path.display()
            )));
        }

        let target_dir = &self.relative_to_root(path);

        // check if modules exist
        let relative_target = target_dir
            .strip_prefix(self.root())
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_str().unwrap())
            .collect::<Vec<_>>()
            .join("-");
        let name_pattern = if module_types.len() == 1 {
            format!("^{}$", relative_target)
        } else {
            format!(
                "^{}$",
                module_types
                    .iter()
                    .filter(|t| !t.is_empty())
                    .map(|t| format!("{}-{}", relative_target, t))
                    .collect::<Vec<_>>()
                    .join("|")
                    .as_str()
            )
        };
        debug!("Use {} to filter projects", &name_pattern);
        let filters = self.create_filters().with_name_regex(&name_pattern);
        self.scan(&filters, scan_impacted_projects);
        if !self.is_empty() {
            return Err(Error::from_str(&format!(
                "Projects with same name already exist: {:?}",
                &self.iter()
            )));
        }

        let excludes_files: Vec<_> = excludes.split(',').collect();
        // create them
        if module_types.len() == 1 {
            self.create_one_module(
                &template_path.join(&module_types[0]),
                target_dir,
                &excludes_files,
            )?;
        } else {
            module_types.iter().try_for_each(|t| {
                self.create_one_module(&template_path.join(t), &target_dir.join(t), &excludes_files)
            })?;
        }

        self.scan(&filters, scan_impacted_projects);
        debug!("Add {:?} to the projects", &self.iter());
        self.append_to_default_settings_file()
            .and_then(|f| self.vc().add_path(&f))
    }

    fn create_one_module<P: AsRef<Path>>(
        &self,
        template: &P,
        target: &P,
        excludes_files: &[&str],
    ) -> Result<()> {
        info!(
            "Creating module {:#?} from {:#?}",
            target.as_ref(),
            template.as_ref()
        );

        copy_dir(
            template.as_ref(),
            target.as_ref(),
            self.root().as_path(),
            excludes_files,
        )
        .map_err(|e| Error::new("Can't copy template directory", e))?;

        debug!("Add files to version control system");
        self.vc().add_path(target.as_ref())
    }

    fn relative_to_root(&self, path: &str) -> PathBuf {
        let mut pb = PathBuf::from(path);
        if pb.is_relative() {
            pb = self.root().join(pb);
        }
        pb
    }
}

fn copy_dir(
    from_absolate: &Path,
    to_absolute: &Path,
    root: &Path,
    excludes_files: &[&str],
) -> io::Result<()> {
    debug!(
        "Coping template from {:#?} to {:#?}",
        from_absolate, to_absolute
    );

    let to = to_absolute.strip_prefix(root).unwrap().to_str().unwrap();
    let package_path = package_name(to).replace('.', "/");
    debug!("Package path is {}", &package_path);

    let feature_name = feature_name(to);
    debug!("Feature name is {}", &feature_name);
    let target_path = |path: &Path| {
        to_absolute.join(path.strip_prefix(from_absolate).unwrap().iter().fold(
            PathBuf::new(),
            |mut pb, c| {
                let component = c.to_str().unwrap();
                pb.push(
                    component
                        .replace("FeatureName", feature_name.as_str())
                        .replace("template-feature", package_path.as_str())
                        .as_str(),
                );
                pb
            },
        ))
    };

    let tokens = &create_tokens(to);
    visit_dirs(from_absolate, &|p: &Path| {
        if p.file_name()
            .and_then(|f| f.to_str())
            .map_or(false, |f| excludes_files.iter().any(|name| **name == *f))
        {
            trace!("Skip file: {:?}", p);
            Ok(())
        } else if p.is_dir() {
            let d = target_path(p);
            trace!("Creating directory: {:?}", d);
            fs::create_dir_all(d)
        } else if p.is_file() {
            let f = target_path(p);
            trace!("Copying file {:?} to {:?}", p, f);
            if let Some(d) = f.parent() {
                fs::create_dir_all(d)?;
            }

            fs::read_to_string(p).and_then(|content| {
                fs::write(
                    f,
                    tokens
                        .iter()
                        .fold(content, |acc: String, (k, v): (&String, &String)| {
                            acc.replace(k.as_str(), v.as_str())
                        }),
                )
            })
        } else {
            warn!("File type of {:?} is not supported!", p);
            Ok(())
        }
    })
}
fn visit_dirs(dir: &Path, cb: &impl Fn(&Path) -> io::Result<()>) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            cb(&path)?;
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            }
        }
    }
    Ok(())
}

fn create_tokens(dir: &str) -> HashMap<String, String> {
    let mut tokens = HashMap::new();
    tokens.insert("#{featureName}".to_string(), feature_name(dir));
    tokens.insert("#{packageName}".to_string(), package_name(dir));

    for (k, v) in &tokens {
        info!("Generated token: {} = {}", k, v);
    }
    tokens
}

fn feature_name(dir: &str) -> String {
    dir.replace('/', "-")
        .split('-')
        .map(|w| {
            let (f, r) = w.split_at(1);
            [f.to_ascii_uppercase(), r.to_string()].join("")
        })
        .collect::<Vec<_>>()
        .join("")
}

fn package_name(dir: &str) -> String {
    dir.to_ascii_lowercase()
        .replace("feature-", "feature.")
        .replace('-', "")
        .replace('/', ".")
        .replace(".domain", "")
}
