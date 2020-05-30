use super::projects::*;
use crate::error::*;
use log::{info, warn};
use std::fs::{remove_dir_all, remove_file};

impl Projects {
    pub fn open(&self, clean: bool) -> Result<()> {
        self.add_to_default_settings_file().map_err(|e| {
            warn!("Can't add projects to the settings file caused by {:?}", e);
            e
        })?;
        let ret = if clean && {
            info!("Cleaning IDE cache");
            let root = self.root_project();
            [
                remove_dir_all(root.join(".idea")),
                remove_file(root.join("root-project.iml")),
                remove_dir_all(root.join("build")),
            ]
            .iter()
            .any(std::result::Result::is_err)
        } {
            Err(Error::from_str("Can't clean the cache"))
        } else {
            Ok(())
        };

        info!("Done. Please open the root-project using Your IDE");
        ret
    }
}
