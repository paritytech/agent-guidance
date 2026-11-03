//! Helpers shared by the loader and installer tests.

use std::{path::Path, process::Command};
use tempfile::TempDir;

/// Creates a temporary directory below Cargo's scratch directory for integration tests.
pub fn tempdir() -> TempDir {
	tempfile::Builder::new()
		.prefix("test-")
		.tempdir_in(env!("CARGO_TARGET_TMPDIR"))
		.expect("creating a temporary directory")
}

/// Isolates `command` from the user's and the system's git configuration.
///
/// This keeps settings such as `insteadOf` rewrites, commit signing and global excludes from
/// changing test results, and gives commits a fixed identity. Repository discovery stops at
/// Cargo's scratch directory, which itself lies inside this repository's checkout.
pub fn isolate(command: &mut Command) -> &mut Command {
	command
		.env("GIT_CEILING_DIRECTORIES", env!("CARGO_TARGET_TMPDIR"))
		.env("GIT_CONFIG_GLOBAL", "/dev/null")
		.env("GIT_CONFIG_NOSYSTEM", "1")
		.env("XDG_CONFIG_HOME", "/nonexistent/agent-guidance-test")
		.env("GIT_AUTHOR_NAME", "Test")
		.env("GIT_AUTHOR_EMAIL", "test@example.com")
		.env("GIT_COMMITTER_NAME", "Test")
		.env("GIT_COMMITTER_EMAIL", "test@example.com")
}

/// Runs an isolated git command in `dir`, panics when it fails, and returns its stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
	let output = isolate(Command::new("git").arg("-C").arg(dir).args(args))
		.output()
		.expect("running git");
	assert!(
		output.status.success(),
		"git {args:?} failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).expect("git output is UTF-8")
}
