use super::*;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

impl Projects {
    pub fn create(&mut self, name: &str, module_type: &str) -> Result<()> {
        let modules = match module_type.to_lowercase().as_str() {
            "feature" => vec!["domain", "android"],
            "android" => vec!["android"],
            "kotlin" => vec!["domain"],
            _ => {
                return Err(Error::from_str(&format!(
                    "Not supported module type: {}",
                    module_type
                )));
            }
        };
        let name_pattern = if modules.len() == 1 {
            format!("^{}$", name)
        } else {
            format!(
                "^{}$",
                modules
                    .iter()
                    .map(|t| format!("{}-{}", name, t))
                    .collect::<Vec<_>>()
                    .join("|")
                    .as_str()
            )
        };
        debug!("Use {} to filter projects", &name_pattern);
        let filters = self.create_filters().with_name_regex(&name_pattern);
        self.scan(&filters);
        if !self.is_empty() {
            return Err(Error::from_str(&format!(
                "Projects with same name already exist: {:?}",
                &self.iter()
            )));
        }

        let root = &self.vc().root();
        let path = Path::new(name).to_path_buf();
        let template_path = self.templates_dir();

        info!("Creating module {} as {}", name, module_type);

        modules
            .iter()
            .map(|m| {
                (
                    if modules.len() == 1 {
                        path.clone()
                    } else {
                        path.join(m)
                    },
                    template_path.join(m),
                )
            })
            .try_for_each(|(to, from)| copy_dir(&from, &to, root))
            .map_err(|e| Error::new("Can't copy template directory", e))?;

        debug!("Add files to version control system");
        self.vc().add_path(Path::new(name))?;

        self.scan(&filters);
        debug!("Add {:?} to the projects", &self.iter());
        self.append_to_default_settings_file()
    }
}

fn copy_dir(from_absolate: &Path, to_path: &Path, root: &Path) -> io::Result<()> {
    debug!(
        "Coping template from {:?} to {:?}",
        &from_absolate, &to_path
    );

    let to = to_path.to_str().unwrap();
    let package_path = package_name(to).replace(".", "/");
    debug!("Package path is {}", &package_path);

    let feature_name = feature_name(to);
    debug!("Feature name is {}", &feature_name);
    let to_absolute = root.join(to_path);
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
        if p.is_dir() {
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
    dir.replace("/", "-")
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
        .replace("-", "")
        .replace("/", ".")
        .replace(".domain", "")
}
