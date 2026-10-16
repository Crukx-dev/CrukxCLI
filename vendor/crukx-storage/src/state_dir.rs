use std::path::{Path, PathBuf};

/// Resolves where Crukx reads/writes local state. Mirrors the TS CLI:
/// `$CRUKX_STATE_DIR` if set, else `<cwd>/.crukx`.
#[derive(Debug, Clone)]
pub struct StateDir {
    root: PathBuf,
}

impl StateDir {
    /// Not unit-tested directly — it reads a process-global env var, which
    /// races under parallel test execution. The path-joining logic it
    /// delegates to (`StateDir::at` + the `*_dir` methods below) is what's
    /// actually tested.
    pub fn resolve(cwd: &Path) -> Self {
        let root = std::env::var_os("CRUKX_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| cwd.join(".crukx"));
        StateDir { root }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        StateDir { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn contracts_dir(&self) -> PathBuf {
        self.root.join("contracts")
    }

    pub fn evidence_dir(&self) -> PathBuf {
        self.root.join("evidence")
    }

    pub fn evaluators_dir(&self) -> PathBuf {
        self.root.join("evaluators")
    }

    pub fn security_dir(&self) -> PathBuf {
        self.root.join("security")
    }

    pub fn events_dir(&self) -> PathBuf {
        self.root.join("events")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdirectories_join_under_the_given_root() {
        let state = StateDir::at("/tmp/example/.crukx");
        assert_eq!(state.root(), Path::new("/tmp/example/.crukx"));
        assert_eq!(
            state.contracts_dir(),
            Path::new("/tmp/example/.crukx/contracts")
        );
        assert_eq!(
            state.evidence_dir(),
            Path::new("/tmp/example/.crukx/evidence")
        );
        assert_eq!(
            state.evaluators_dir(),
            Path::new("/tmp/example/.crukx/evaluators")
        );
        assert_eq!(
            state.security_dir(),
            Path::new("/tmp/example/.crukx/security")
        );
        assert_eq!(state.events_dir(), Path::new("/tmp/example/.crukx/events"));
    }
}
