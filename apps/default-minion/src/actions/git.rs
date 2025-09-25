use std::path::Path;

use git2::{build::RepoBuilder, Repository};
use url::Url;

pub struct Repo {
    repo: Repository,
    branch: String,
}

impl Repo {
    /// Clone (and configure) a git repository
    pub fn clone<P: AsRef<Path>>(
        clone_to: P,
        url: &Url,
        branch: &str,
        user_name: &str,
        user_email: &str,
    ) -> Self {
        let mut repo_builder = RepoBuilder::new();
        repo_builder.branch(branch);
        let repo = repo_builder.clone(url.as_str(), clone_to.as_ref()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", user_name).unwrap();
        config.set_str("user.email", user_email).unwrap();

        Self { repo, branch: branch.to_owned() }
    }

    pub fn commit_and_push(&self) {
        let mut index = self.repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        let oid = index.write_tree().unwrap();
        let tree = self.repo.find_tree(oid).unwrap();
        let head = self.repo.head().unwrap();
        let parent = self.repo.find_commit(head.target().unwrap()).unwrap();
        let sig = self.repo.signature().unwrap();
        let message = "Commit from minionrt";
        self.repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent]).unwrap();
        let mut remote = self.repo.find_remote("origin").unwrap();
        remote
            .push(&[format!("refs/heads/{}:refs/heads/{}", self.branch, self.branch)], None)
            .unwrap();
    }
}
