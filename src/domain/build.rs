use std::process::Command;

use super::*;
use std::path::Path;

impl Projects {
    pub fn build(&self, gradle_commands: &[String], verbose: usize) -> Result<()> {
        let settings_file = self.root_project().join("build.settings.gradle.kts");
        trace!("Adding projects:");
        self.iter().for_each(|l| trace!("{:?}", l));

        if self.is_empty() {
            info!("No project need to run");
            Ok(())
        } else {
            self.create_settings_for_subprojects(self.iter(), &settings_file)?;
            self.build_projects(&settings_file, gradle_commands, verbose)
                .map_err(|e| Error::new("Can't start build process", e))?
                .wait()
                .map_or_else(
                    |e| Err(Error::new("Build process stop failed", e)),
                    |s| {
                        if s.success() {
                            Ok(())
                        } else {
                            Err(Error::from_str("Build process failed"))
                        }
                    },
                )
        }
    }

    fn build_projects(
        &self,
        settings_file: &Path,
        gradle_commands: &[String],
        verbose: usize,
    ) -> std::io::Result<std::process::Child> {
        let mut filtered_commands: Vec<_> = gradle_commands
            .iter()
            .filter_map(|c| {
                if !c.contains(':') {
                    Some(c.as_str())
                } else {
                    let mut names = c.rsplitn(2, ':');
                    names.next();
                    let project_name = names.next().unwrap().trim_start_matches(':');
                    if self.iter().any(|p| p.name == project_name) {
                        Some(c.as_str())
                    } else {
                        info!("Project {} not found, so skip command {}", project_name, c);
                        None
                    }
                }
            })
            .collect();
        if filtered_commands.is_empty() {
            filtered_commands.push("projects");
        }

        info!(
            "Start run gradle {} on {}",
            filtered_commands.join(" "),
            &settings_file.to_str().unwrap()
        );

        let cmd = self.gradle_cmd();
        let gradle_cmd = cmd.split_whitespace().collect::<Vec<_>>();
        let mut cmd = Command::new(gradle_cmd[0]);
        match verbose {
            0 => cmd.arg("-w"),
            1 => &mut cmd,
            2 => cmd.arg("-i"),
            _ => cmd.arg("-d"),
        }
        .args(["-c", settings_file.to_str().unwrap()])
        .args(&gradle_cmd[1..]);

        cmd.args(filtered_commands).spawn()
    }
}
