use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitTouchPoint {
	pub commit_id: String,
	pub author: String,
	pub summary: String,
	pub touched_at_unix: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileGitInsight {
	pub file_path: String,
	pub churn_commits: usize,
	pub last_touch: Option<GitTouchPoint>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ProjectGitInsights {
	pub available: bool,
	pub repo_root: Option<String>,
	pub hot_files: Vec<FileGitInsight>,
	pub stable_files: Vec<FileGitInsight>,
	pub recent_authors: Vec<String>,
	pub summary: Vec<String>,
}

pub struct GitArchaeologist {
    repo: Repository,
}

impl GitArchaeologist {
    /// Binds dynamically to the active project workspace root to scrape deep contextual git history.
    pub fn open(workspace_root: &Path) -> Result<Self> {
        let repo = Repository::open(workspace_root)
            .context("Failed to attach Git2 mapping inside target workspace")?;

        Ok(Self { repo })
    }

	pub fn repo_root(&self) -> Option<String> {
		self.repo
			.workdir()
			.map(|p| p.to_string_lossy().to_string())
	}

    /// Evaluates the true structural churn of a file. High churn indicates deep complexity or a
    /// severely unstable architectural region that Memix needs to watch carefully.
    pub fn calculate_file_churn(&self, file_path: &Path, limit: usize) -> Result<usize> {
		let target_path = self.repo_relative_path(file_path)?;
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;

        let mut churn_count = 0;

        for oid_result in revwalk.take(limit) {
            let oid = oid_result?;
            let commit = self.repo.find_commit(oid)?;

            // Analyze tree diffs checking if `file_path` mutated natively
            if let Ok(parent) = commit.parent(0) {
                let current_tree = commit.tree()?;
                let parent_tree = parent.tree()?;

                let diff =
                    self.repo
                        .diff_tree_to_tree(Some(&parent_tree), Some(&current_tree), None)?;

                let mut mutated = false;
                diff.print(git2::DiffFormat::NameOnly, |delta, _hunk, _line| {
                    if let Some(path) = delta.new_file().path() {
                        if path.to_string_lossy() == target_path {
                            mutated = true;
                        }
                    } else if let Some(path) = delta.old_file().path() {
						if path.to_string_lossy() == target_path {
							mutated = true;
						}
                    }
                    true
                })?;

                if mutated {
                    churn_count += 1;
                }
            }
        }

        Ok(churn_count)
    }

	pub fn last_touch(&self, file_path: &Path, limit: usize) -> Result<Option<GitTouchPoint>> {
		let target_path = self.repo_relative_path(file_path)?;
		let mut revwalk = self.repo.revwalk()?;
		revwalk.push_head()?;

		for oid_result in revwalk.take(limit) {
			let oid = oid_result?;
			let commit = self.repo.find_commit(oid)?;

			if let Ok(parent) = commit.parent(0) {
				let current_tree = commit.tree()?;
				let parent_tree = parent.tree()?;
				let diff = self
					.repo
					.diff_tree_to_tree(Some(&parent_tree), Some(&current_tree), None)?;

				let mut touched = false;
				diff.print(git2::DiffFormat::NameOnly, |delta, _hunk, _line| {
					if let Some(path) = delta.new_file().path() {
						if path.to_string_lossy() == target_path {
							touched = true;
						}
					} else if let Some(path) = delta.old_file().path() {
						if path.to_string_lossy() == target_path {
							touched = true;
						}
					}
					true
				})?;

				if touched {
					let author = commit.author();
					return Ok(Some(GitTouchPoint {
						commit_id: commit.id().to_string(),
						author: author.name().unwrap_or("unknown").to_string(),
						summary: commit.summary().unwrap_or("No commit summary").to_string(),
						touched_at_unix: commit.time().seconds(),
					}));
				}
			}
		}

		Ok(None)
	}

	pub fn project_insights(&self, files: &[String], limit: usize) -> Result<ProjectGitInsights> {
		let mut insights = Vec::new();
		for file in files {
			let path = Path::new(file);
			let churn_commits = self.calculate_file_churn(path, limit).unwrap_or(0);
			let last_touch = self.last_touch(path, limit).unwrap_or(None);
			insights.push(FileGitInsight {
				file_path: file.clone(),
				churn_commits,
				last_touch,
			});
		}

		insights.sort_by(|a, b| b.churn_commits.cmp(&a.churn_commits).then_with(|| a.file_path.cmp(&b.file_path)));

		let hot_files = insights.iter().take(5).cloned().collect::<Vec<_>>();
		let mut stable_files = insights.clone();
		stable_files.sort_by(|a, b| a.churn_commits.cmp(&b.churn_commits).then_with(|| a.file_path.cmp(&b.file_path)));
		stable_files.truncate(5);

		let mut recent_authors = insights
			.iter()
			.filter_map(|item| item.last_touch.as_ref().map(|touch| touch.author.clone()))
			.collect::<Vec<_>>();
		recent_authors.sort();
		recent_authors.dedup();
		recent_authors.truncate(5);

		let summary = vec![
			format!("tracked_files={}", insights.len()),
			format!("repo_root={}", self.repo_root().unwrap_or_else(|| "unknown".to_string())),
			format!("max_churn={}", hot_files.first().map(|f| f.churn_commits).unwrap_or(0)),
		];

		Ok(ProjectGitInsights {
			available: true,
			repo_root: self.repo_root(),
			hot_files,
			stable_files,
			recent_authors,
			summary,
		})
	}

	fn repo_relative_path(&self, file_path: &Path) -> Result<String> {
		let normalized = if file_path.is_absolute() {
			if let Some(workdir) = self.repo.workdir() {
				let canonical_workdir = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());
				let canonical_file = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
				canonical_file
					.strip_prefix(&canonical_workdir)
					.or_else(|_| canonical_file.strip_prefix(workdir))
					.unwrap_or(canonical_file.as_path())
					.to_path_buf()
			} else {
				file_path.to_path_buf()
			}
		} else {
			file_path.to_path_buf()
		};

		Ok(normalized.to_string_lossy().replace('\\', "/"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use git2::{Repository, Signature};
	use std::fs;

	fn commit_file(repo: &Repository, file_path: &Path, message: &str) {
		let workdir = repo.workdir().unwrap().canonicalize().unwrap();
		let normalized_file = file_path.canonicalize().unwrap();
		let rel = normalized_file.strip_prefix(&workdir).unwrap();
		let mut index = repo.index().unwrap();
		index.add_path(rel).unwrap();
		index.write().unwrap();
		let tree_id = index.write_tree().unwrap();
		let tree = repo.find_tree(tree_id).unwrap();
		let sig = Signature::now("Memix Test", "memix@example.com").unwrap();

		let parent = repo.head().ok().and_then(|head| head.target()).and_then(|oid| repo.find_commit(oid).ok());
		if let Some(parent) = parent.as_ref() {
			repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[parent]).unwrap();
		} else {
			repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[]).unwrap();
		}
	}

	#[test]
	fn calculates_churn_for_absolute_workspace_paths() {
		let repo_dir = std::env::temp_dir().join(format!("memix-git-test-{}", uuid::Uuid::new_v4()));
		fs::create_dir_all(repo_dir.join("src")).unwrap();
		let repo = Repository::init(&repo_dir).unwrap();
		let tracked_file = repo_dir.join("src").join("module.rs");

		fs::write(&tracked_file, "fn alpha() {}\n").unwrap();
		commit_file(&repo, &tracked_file, "initial");

		fs::write(&tracked_file, "fn alpha() { println!(\"hi\"); }\n").unwrap();
		commit_file(&repo, &tracked_file, "update");

		let archaeologist = GitArchaeologist::open(&repo_dir).unwrap();
		let churn = archaeologist.calculate_file_churn(&tracked_file, 20).unwrap();
		let touch = archaeologist.last_touch(&tracked_file, 20).unwrap();

		assert!(churn >= 1);
		assert!(touch.is_some());
		assert_eq!(touch.unwrap().summary, "update");

		let _ = fs::remove_dir_all(&repo_dir);
	}
}
