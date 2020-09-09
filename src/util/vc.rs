use crate::domain::projects::VersionControl;
use crate::error::*;
use git2::{
    BranchType, DiffDelta, DiffOptions, FetchOptions, IndexAddOption, PushOptions, Remote,
    RemoteCallbacks, Repository,
};
use log::{debug, warn};

pub struct GitVersionControl {
    repository: Repository,
}
impl GitVersionControl {
    pub fn new() -> Self {
        Self {
            repository: Self::open_repo().unwrap(),
        }
    }
    fn open_repo() -> Result<Repository> {
        Repository::discover(".").
        map_err(|e|{
            warn!("Can't open the repository: {:?}", e);
            Error::new("Can't open the repository, please check out if current directory is under a git repository", e)})
    }

    fn origin_remote(&self) -> Result<Remote> {
        self.repository
            .find_remote("origin")
            .map_err(|e| Error::new("The origin remote is not set", e))
    }
}
impl VersionControl for GitVersionControl {
    fn root(&self) -> std::path::PathBuf {
        self.repository.path().parent().unwrap().to_path_buf()
    }

    fn remote_branch(&self) -> Result<String> {
        debug!("Getting current remote branch");

        self.repository
            .branches(Some(BranchType::Local))
            .map_err(|e| Error::new("Can't list branches", e))
            .and_then(|bs| {
                bs.filter_map(std::result::Result::ok)
                    .find_map(|(b, _)| {
                        if b.is_head() {
                            Some(
                                b.upstream()
                                    .map_err(|e| {
                                        Error::new("No upstream branch for the local branh", e)
                                    })
                                    .map(|b| b.name().unwrap().unwrap().to_string()),
                            )
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        Err(Error::from_str("Can't find the branch pointed by HEAD"))
                    })
            })
    }

    fn diff_files(&self, commit: &str) -> Result<Vec<String>> {
        debug!("Getting diff changes since {}", commit);

        let tree = self
            .repository
            .revparse_single(commit)
            .map(|r| r.id())
            .or_else(|_| {
                self.repository
                    .refname_to_id(&format!("refs/remotes/{}", commit))
            })
            .and_then(|id| self.repository.find_commit(id))
            .and_then(|c| c.tree())
            .map_err(|e| {
                Error::new(
                    &format!("Can't find out the tree reference by {}", &commit),
                    e,
                )
            })?;

        self.repository
            .diff_tree_to_workdir(
                Some(&tree),
                Some(DiffOptions::new().include_untracked(true)),
            )
            .map_err(|e| {
                Error::new(
                    &format!("Can't find out the diff with reference {}", &commit),
                    e,
                )
            })
            .and_then(|d| {
                let mut files = vec![];
                d.foreach(
                    &mut |diff_delta: DiffDelta, _| {
                        if let Some(p) = diff_delta.old_file().path() {
                            files.push(p.to_str().unwrap().to_string());
                        }
                        true
                    },
                    None,
                    None,
                    None,
                )
                .map_err(|e| Error::new("Can't iterate diff", e))
                .map(|_| {
                    debug!("Diff files: {:?}", &files);
                    files
                })
            })
    }

    fn add_path(&self, path: &std::path::Path) -> Result<()> {
        debug!("Add {:?} to the repository", path);

        let relative_path = path
            .strip_prefix(self.root())
            .unwrap_or(path)
            .to_str()
            .unwrap();
        self.repository.index().map_or_else(
            |e| Err(Error::new("Can't get the index of the repo", e)),
            |mut i| {
                i.add_all(
                    &[relative_path, &format!("{}/*", relative_path)],
                    IndexAddOption::DEFAULT,
                    None,
                )
                .and_then(|_| i.write())
                .map_err(|e| Error::new("Can't add path to index", e))
            },
        )
    }

    fn commit(&self, message: &str) -> Result<()> {
        debug!("Commit changes with message: {}", message);

        let sig = self
            .repository
            .signature()
            .map_err(|e| Error::new("Can't get the user info from git config", e))?;
        let parent = self
            .repository
            .find_reference("HEAD")
            .and_then(|r| r.peel_to_commit())
            .map_err(|e| Error::new("Can't find HEAD", e))?;
        let tree = self
            .repository
            .index()
            .and_then(|mut i| i.write_tree())
            .and_then(|id| self.repository.find_tree(id))
            .map_err(|e| Error::new("Can't get index tree", e))?;
        self.repository
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .map(|_| ())
            .map_err(|e| Error::new("Can't create a commit", e))
    }

    fn get_push_url(&self) -> Result<String> {
        self.origin_remote().and_then(|o| {
            o.url().map_or_else(
                || Err(Error::from_str("The push url is not a valid string")),
                |s| Ok(s.to_string()),
            )
        })
    }

    fn log(&self, range: &str, include_body: bool) -> Result<Vec<String>> {
        debug!("Generate diff log in range {}", range);

        let repo = &self.repository;
        let mut revwalk = repo
            .revwalk()
            .map_err(|e| Error::new("Can't walk commit graph", e))?;
        revwalk
            .push_range(range)
            .map_err(|e| Error::new(&format!("Can't push commit range {}", range), e))?;

        let mut v = vec![];
        for id in revwalk {
            match id.and_then(|i| repo.find_commit(i)) {
                Ok(c) => v.push(
                    if include_body {
                        c.message()
                    } else {
                        c.summary()
                    }
                    .unwrap()
                    .to_string(),
                ),
                Err(e) => warn!("Can't find commit caused by {:?}", e),
            }
        }
        debug!("Logs:\n{}", v.join("\n"));
        Ok(v)
    }

    fn fetch(&self, branch: &str) -> Result<()> {
        debug!("Fetch changes from remote branch: {}", branch);

        self.origin_remote().and_then(|mut o| {
            o.fetch(
                &[branch],
                Some(FetchOptions::new().remote_callbacks(create_remote_callback(branch))),
                None,
            )
            .map_err(|e| Error::new("Can't fetch data from origin", e))
        })
    }

    fn push(&self, branch: &str) -> Result<()> {
        debug!("Push changes to remote branch: {}", branch);

        self.origin_remote().and_then(|mut o| {
            o.push(
                &[&format!("HEAD:refs/heads/{}", branch)],
                Some(PushOptions::new().remote_callbacks(create_remote_callback(branch))),
            )
            .map_err(|e| Error::new("Can't push data to origin", e))
        })
    }

    fn merge(&self, remote_branch: &str, dry_run: bool) -> Result<()> {
        let commit = self
            .repository
            .refname_to_id(&format!("refs/remotes/origin/{}", remote_branch))
            .and_then(|r| self.repository.find_annotated_commit(r))
            .map_err(|e| {
                Error::new(
                    &format!("Can't get commit from origin/{}", remote_branch),
                    e,
                )
            })?;
        if dry_run {
            self.repository
                .merge_analysis(&[&commit])
                .map_err(|e| Error::new(&format!("Can't merge {}", remote_branch), e))
                .map(|_| ())
        } else {
            self.repository
                .merge(&[&commit], None, None)
                .map_err(|e| Error::new(&format!("Can't merge {}", remote_branch), e))
                .map(|_| ())
        }
    }
}
fn create_remote_callback(_: &str) -> RemoteCallbacks {
    use git2::Cred;
    use std::env;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, _allowed_types| {
        let user = username_from_url.unwrap();
        Cred::ssh_key_from_agent(user).or_else(|_| {
            Cred::ssh_key(
                user,
                None,
                std::path::Path::new(&format!("{}/.ssh/id_rsa", env::var("HOME").unwrap())),
                None,
            )
        })
    });
    callbacks
}
