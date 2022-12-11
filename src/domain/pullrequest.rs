use super::*;
use crate::api::submit_pull_request as pr_api;
use crate::util::shell::run_sh;

#[derive(Debug)]
pub struct Request {
    pub title: String,
    pub description: String,
    pub branch_name: String,
    pub to_branch: String,
    pub reviewers: Vec<String>,
}

impl Projects {
    pub fn create_pull_request(
        &mut self,
        mut request: Request,
        auth: Auth,
        verbose: usize,
        open: bool,
        triggers_file: Option<String>,
    ) -> Result<()> {
        check_request(&mut request, self.vc())?;
        check_conflicts(&request.to_branch, self.vc())?;

        info!("Build modules");
        self.scan(
            &self.create_filters().since_commit(
                self.vc(),
                &format!("origin/{}", &request.to_branch),
                triggers_file.as_deref(),
            ),
            true,
        );
        self.build(&["build".into()], None, verbose)?;
        info!("Build complete");

        push_changes(&request.branch_name, self.vc())?;

        self.vc()
            .get_push_url()
            .and_then(|url| submit_pull_request(&request, &url, auth, open))
    }
}

fn check_request(request: &mut Request, vc: &dyn VersionControl) -> Result<()> {
    check_branch_name(request, vc)?;

    if request.title.is_empty() {
        let log = vc.log("HEAD~..HEAD", false)?;
        request.title.push_str(&log.join("\n"));
        trace!("Use default title {} since no one is given", &request.title);
    }

    if request.description.is_empty() {
        let log = vc.log(&format!("origin/{}..HEAD", &request.to_branch), true)?;
        request.description.push_str(&log.join("\n"));
        trace!(
            "Use default description {} since no one is given",
            &request.description
        );
    }
    Ok(())
}

fn check_branch_name(request: &mut Request, vc: &dyn VersionControl) -> Result<()> {
    if request.branch_name.is_empty() {
        let remote_branch = vc.remote_branch()?;
        request
            .branch_name
            .push_str(remote_branch.trim_start_matches("origin/"));
    }

    let remote_branch = &request.branch_name;
    if remote_branch == "master" || remote_branch == "develop" {
        warn!(
            "We can Not use {} as the remote branch name, please give another name",
            remote_branch
        );
        return Err(Error::from_str(&format!(
            "Bad remote branch name: {}",
            remote_branch
        )));
    } else {
        trace!("We will use {} as the remote branch", remote_branch);
    }

    Ok(())
}

fn check_conflicts(branch: &str, vc: &dyn VersionControl) -> Result<()> {
    info!("Fetching latest code from remtoes");
    vc.fetch(branch)?;

    info!("Checking conflicts");
    vc.merge(branch, true)
}

fn push_changes(branch: &str, vc: &dyn VersionControl) -> Result<()> {
    info!("Push changes to remote {}", branch);
    vc.push(branch)
}

fn submit_pull_request(request: &Request, push_url: &str, auth: Auth, open: bool) -> Result<()> {
    let url = pr_api(request, push_url, &auth.ask_password_if_none())
        .map_err(|e| Error::new("Can't create a pull request", e))?;

    if open {
        run_sh(&format!("open '{}'", url)).map(|_| ())
    } else {
        Ok(())
    }
}
