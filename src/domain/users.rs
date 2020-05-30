use super::*;

use crate::api::find_users;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::vec::Vec;

#[derive(Clone, Debug)]
pub struct User {
    pub name: String,
    pub display_name: String,
    pub email: String,
}

impl Projects {
    pub fn query_users<P: AsRef<Path>>(
        &self,
        filters: Vec<&str>,
        auth: Auth,
        file: &P,
        append: bool,
    ) -> Result<()> {
        let auth = auth.ask_password_if_none();
        let mut all_users = vec![];
        let push_url = self.vc().get_push_url()?;
        (filters
            .iter()
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .try_for_each(|filter| {
                let result = find_users(&filter, &push_url, &auth)?;
                if result.len() > 1 {
                    warn!(
                        "More than one user identified with given filter: {} \n {:?}",
                        filter, result
                    );
                }

                info!("{:?}", result);

                all_users.extend_from_slice(&result);
                Ok(())
            }) as Result<()>)?;

        let names = all_users
            .iter()
            .map(|u| u.name.as_str())
            .collect::<Vec<_>>()
            .join(";");
        info!("{}\n", names);

        OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(file)
            .and_then(|mut f| writeln!(f, "{};", &names))
            .map_err(|e| Error::new("Can't write result to the file", e))
    }
}
