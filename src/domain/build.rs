use std::process::Command;

use super::*;
use std::path::Path;

impl Projects {
    pub fn build(
        &self,
        gradle_commands: &[String],
        number_of_project_run_together: usize,
        verbose: usize,
    ) -> Result<()> {
        let jobs = self
            .chunks(number_of_project_run_together)
            .enumerate()
            .map(|(i, p)| {
                let settings_file = self
                    .root_project()
                    .join(format!("build.settings{}.gradle.kts", i));
                trace!("Adding projects:");
                p.iter().for_each(|l| trace!("{:?}", l));

                self.create_settings_for_subprojects(p.iter(), &settings_file)
                    .map_or_else(
                        |e| {
                            warn!(
                                "Can't create settings file {:?} caused by {}",
                                &settings_file, e
                            );
                            None
                        },
                        |_| {
                            self.build_projects(&settings_file, gradle_commands, verbose)
                                .map_err(|e| {
                                    warn!("Run build process failed  caused by {:?}", e);
                                    e
                                })
                                .ok()
                        },
                    )
            });
        let all_passed = jobs
            .map(|r| {
                r.map(|mut j| {
                    j.wait().map_or_else(
                        |e| {
                            warn!("Build process stop failed  caused by {:?}", e);
                            None
                        },
                        |s| {
                            trace!("{:?}", &s);
                            s.code()
                        },
                    )
                })
                .flatten()
            })
            .all(|c| c.is_some() && c.unwrap() == 0);

        if !all_passed {
            Err(Error::from_str(
                "Build failed, please check out warning messages!",
            ))
        } else {
            Ok(())
        }
    }

    fn build_projects(
        &self,
        settings_file: &Path,
        gradle_commands: &[String],
        verbose: usize,
    ) -> std::io::Result<std::process::Child> {
        info!(
            "Start run gradle {} on {}",
            gradle_commands.join(" "),
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
        .args(&["-c", settings_file.to_str().unwrap()])
        .args(&gradle_cmd[1..]);

        cmd.args(gradle_commands).spawn()
    }
}
