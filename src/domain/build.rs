use std::process::Command;

use super::*;
use std::path::Path;

impl Projects {
    pub fn build(&self, gradle_commands: &[String], verbose: usize) -> Result<()> {
        let settings_file = self.root_project().join("build.settings.gradle.kts");
        trace!("Adding projects:");
        self.iter().for_each(|l| trace!("{:?}", l));

        self.create_settings_for_subprojects(self.iter(), &settings_file)?;
        self.build_projects(&settings_file, gradle_commands, verbose)
            .map_err(|e| Error::new("Can't start build process", e))?
            .wait()
            .map_or_else(
                |e| Err(Error::new("Build process stop failed", e)),
                |_| Ok(()),
            )
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
