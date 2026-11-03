//! Tests for `tools/templates/.agent-guidance-loader/load.sh`.
//!
//! Every case runs twice: under `/bin/bash` (bash 3.2 on macOS) and under the `bash` found first on
//! `PATH`. A local bare repository stands in for the guidance repository.

mod common;

use common::{git, isolate, tempdir};
use serde_json::Value;
use std::{
	env,
	fs::{self, File},
	io::Read,
	net::TcpListener,
	os::unix::{fs::PermissionsExt, process::CommandExt},
	path::{Path, PathBuf},
	process::{Command, Output, Stdio},
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
	thread,
	time::{Duration, Instant, SystemTime},
};
use tempfile::TempDir;

const LOADER: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/templates/.agent-guidance-loader/load.sh"
);
const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
const MARKER: &str = "Verification marker: test-1";
const UPDATED: &str = "Verification marker: test-2\n";
const OFFLINE: &str = "file:///nonexistent/agent-guidance.git";
const HARNESSES: [&str; 5] = ["claude", "codex", "gemini", "cursor", "copilot"];
const FALLBACK: &str = "Company agent guidance could not be fetched for this session. Follow the \
	agent-guidance instruction in AGENTS.md or CLAUDE.md to read it.";
const OUT_OF_DATE: &str = "; loader out of date, re-run agent-guidance-install";

/// Upper bound for a loader run against a network that accepts connections and never answers.
const HUNG_BUDGET: Duration = Duration::from_secs(6);

/// How long the watchdog lets git run. A run that takes this long although git finished quickly
/// means the watchdog kept the output pipes open.
const WATCHDOG: Duration = Duration::from_secs(4);

/// Upper bound for a run that serves the cache and leaves the fetch to the background.
const BACKGROUND_RUN: Duration = Duration::from_secs(1);

/// Upper bound for waiting on a background fetch.
const BACKGROUND_FETCH: Duration = Duration::from_secs(10);

/// Age the tests give the last-fetch marker: recent enough for a background fetch.
const RECENT: Duration = Duration::from_secs(60 * 60);

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Age of the last-fetch marker after which the loader fetches before serving.
const STALE: Duration = DAY.saturating_mul(7);

/// Age the tests give the last-fetch marker to force a blocking fetch.
const EXPIRED: Duration = DAY.saturating_mul(8);

/// Generates one test per bash for each loader case.
macro_rules! loader_tests {
	($($case:ident),* $(,)?) => {
		$(
			mod $case {
				#[test]
				fn system_bash() {
					super::$case(std::path::Path::new("/bin/bash"));
				}

				#[test]
				fn path_bash() {
					super::$case(&super::which("bash").expect("bash on PATH"));
				}
			}
		)*
	};
}

loader_tests!(
	fresh_clone,
	background_update,
	six_day_marker_does_not_block,
	background_fetch_does_not_block,
	background_fetch_survives_group_kill,
	expired_marker_blocks,
	offline_with_cache,
	offline_without_cache,
	hung_network_without_cache,
	hung_network_with_expired_marker,
	killed_during_clone,
	escaping_without_python,
	concurrent_first_runs,
	committed_guidance_size,
	stale_loader_notice,
	unapplied_fetch_is_reported,
	git_not_found,
	credentials_are_not_shown,
	cache_without_guidance,
	cache_without_git_in_downstream_repository,
	empty_git_directory_heals,
	zero_background_budget_is_ignored,
	ignores_python_modules_in_the_current_directory,
	local_edits_are_discarded_on_update,
	unchanged_remote_adds_no_reflog_entries,
	force_pushed_remote_updates_the_cache,
	relative_invocation_from_the_root,
	background_fetch_times_out,
);

/// Finds `name` in the directories of `PATH`.
fn which(name: &str) -> Option<PathBuf> {
	let path = env::var_os("PATH")?;
	env::split_paths(&path)
		.map(|dir| dir.join(name))
		.find(|candidate| candidate.is_file())
}

/// Lists the names in `dir`, sorted.
fn entries(dir: &Path) -> Vec<String> {
	let mut names: Vec<String> = fs::read_dir(dir)
		.expect("reading a directory")
		.map(|entry| {
			let name = entry.expect("reading a directory entry").file_name();
			name.into_string().expect("UTF-8 file name")
		})
		.collect();
	names.sort();
	names
}

/// Polls `done` every 20 ms and panics when it is still false after `bound`.
fn wait_until(bound: Duration, what: &str, mut done: impl FnMut() -> bool) {
	let start = Instant::now();
	while !done() {
		assert!(
			start.elapsed() < bound,
			"{what} did not happen within {bound:?}"
		);
		thread::sleep(Duration::from_millis(20));
	}
}

/// A server that accepts connections and never answers.
struct StalledServer {
	url: String,
	accepted: Arc<AtomicUsize>,
	closed: Arc<AtomicUsize>,
}

impl StalledServer {
	fn start() -> StalledServer {
		let listener = TcpListener::bind("127.0.0.1:0").expect("binding a local port");
		let port = listener.local_addr().expect("reading the address").port();
		let accepted = Arc::new(AtomicUsize::new(0));
		let closed = Arc::new(AtomicUsize::new(0));
		let (accepted_by_server, closed_by_server) = (accepted.clone(), closed.clone());
		thread::spawn(move || {
			for stream in listener.incoming().flatten() {
				accepted_by_server.fetch_add(1, Ordering::SeqCst);
				let closed = closed_by_server.clone();
				// Read until the client goes away, without ever writing.
				thread::spawn(move || {
					let mut stream = stream;
					let mut buffer = [0; 4096];
					while stream.read(&mut buffer).is_ok_and(|read| read > 0) {}
					closed.fetch_add(1, Ordering::SeqCst);
				});
			}
		});
		StalledServer {
			url: format!("http://127.0.0.1:{port}/x.git"),
			accepted,
			closed,
		}
	}

	/// Waits until a client connected and every client disconnected.
	fn wait_for_clients_to_leave(&self) {
		wait_until(BACKGROUND_FETCH, "the stalled fetch ending", || {
			let accepted = self.accepted.load(Ordering::SeqCst);
			accepted > 0 && self.closed.load(Ordering::SeqCst) == accepted
		});
	}
}

/// A guidance remote and a downstream repository root with the loader installed.
struct Fixture {
	_dir: TempDir,
	base: PathBuf,
	work: PathBuf,
	remote: PathBuf,
	root: PathBuf,
}

impl Fixture {
	/// Creates an empty downstream root and a remote whose `GUIDANCE.md` holds `guidance`.
	///
	/// The remote also holds this repository's `loader-version`.
	fn new(guidance: &[u8]) -> Fixture {
		let dir = tempdir();
		let base = fs::canonicalize(dir.path()).expect("canonicalizing the temporary directory");
		let fixture = Fixture {
			_dir: dir,
			work: base.join("work"),
			remote: base.join("remote.git"),
			root: base.join("downstream"),
			base,
		};
		fs::create_dir(&fixture.work).expect("creating the work repository");
		git(&fixture.work, &["init", "-q", "-b", "main"]);
		git(
			&fixture.base,
			&["init", "-q", "--bare", "-b", "main", "remote.git"],
		);
		fs::create_dir_all(fixture.root.join(".agent-guidance-loader"))
			.expect("creating the downstream root");
		fs::copy(LOADER, fixture.loader()).expect("copying the loader");
		let version = fs::read(format!("{REPO}/loader-version")).expect("reading loader-version");
		fixture.publish(&[("GUIDANCE.md", guidance), ("loader-version", &version)]);
		fixture
	}

	/// Commits `files` to the work repository and pushes them to the remote.
	fn publish(&self, files: &[(&str, &[u8])]) {
		for (path, contents) in files {
			let full = self.work.join(path);
			fs::create_dir_all(full.parent().expect("a file has a parent"))
				.expect("creating a directory");
			fs::write(full, contents).expect("writing a guidance file");
		}
		self.push();
	}

	fn push(&self) {
		git(&self.work, &["add", "-A"]);
		git(&self.work, &["commit", "-q", "-m", "update"]);
		let remote = self.remote.to_str().expect("UTF-8 path");
		git(&self.work, &["push", "-q", remote, "main"]);
	}

	fn loader(&self) -> PathBuf {
		self.root.join(".agent-guidance-loader/load.sh")
	}

	fn url(&self) -> String {
		format!("file://{}", self.remote.display())
	}

	/// A URL for the remote whose fetches take at least one second.
	fn slow_url(&self) -> String {
		let script = self.base.join("slow-upload-pack");
		fs::write(&script, "#!/bin/sh\nsleep 1\nexec git upload-pack \"$1\"\n")
			.expect("writing the upload-pack wrapper");
		fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("setting the mode");
		format!("ext::{} {}", script.display(), self.remote.display())
	}

	fn cache(&self) -> PathBuf {
		self.root.join(".agent-guidance")
	}

	fn marker(&self) -> PathBuf {
		self.cache().join(".git/agent-guidance-last-fetch")
	}

	/// The modification time of the last-fetch marker.
	fn marker_time(&self) -> SystemTime {
		fs::metadata(self.marker())
			.and_then(|metadata| metadata.modified())
			.expect("reading the last-fetch marker")
	}

	/// Makes the last fetch look `age` old and returns the new modification time.
	fn age_marker(&self, age: Duration) -> SystemTime {
		let time = SystemTime::now() - age;
		File::options()
			.write(true)
			.open(self.marker())
			.and_then(|file| file.set_modified(time))
			.expect("setting the modification time of the last-fetch marker");
		time
	}

	/// Waits until a fetch touched the last-fetch marker after `since`.
	fn wait_for_fetch(&self, since: SystemTime) {
		wait_until(BACKGROUND_FETCH, "the background fetch", || {
			self.marker_time() > since
		});
	}

	/// The context the loader injects for `guidance`, which must end with one newline.
	fn context(&self, guidance: &str) -> String {
		let body = guidance
			.strip_suffix('\n')
			.expect("guidance ends with a newline");
		format!(
			"Company agent guidance follows. Files it links to live under `{}/`.\n\n{body}",
			self.cache().display()
		)
	}

	/// The abbreviated commit the cache has checked out, as the status line shows it.
	fn short_head(&self) -> String {
		git(&self.cache(), &["rev-parse", "--short", "HEAD"])
			.trim_end()
			.to_owned()
	}

	fn command(&self, bash: &Path, url: &str, harness: Option<&str>) -> Command {
		self.command_running(bash, &self.loader(), url, harness)
	}

	/// A command that runs the loader through the path `script`.
	fn command_running(
		&self,
		bash: &Path,
		script: &Path,
		url: &str,
		harness: Option<&str>,
	) -> Command {
		let mut command = Command::new(bash);
		isolate(&mut command)
			.arg(script)
			.args(harness)
			.env("AGENT_GUIDANCE_URL", url)
			.env("GIT_CONFIG_COUNT", "1")
			.env("GIT_CONFIG_KEY_0", "protocol.ext.allow")
			.env("GIT_CONFIG_VALUE_0", "always")
			.stdin(Stdio::null());
		command
	}

	/// Runs the loader once, without waiting for a background fetch it starts.
	fn run_once(&self, bash: &Path, url: &str, harness: Option<&str>) -> Run {
		let start = Instant::now();
		let output = self
			.command(bash, url, harness)
			.output()
			.expect("running the loader");
		Run {
			output,
			elapsed: start.elapsed(),
		}
	}

	/// Runs the loader and, when it starts a background fetch from the fixture's remote, waits
	/// for that fetch to finish, so that runs do not overlap with fetches.
	fn run(&self, bash: &Path, url: &str, harness: Option<&str>) -> Run {
		let recent = self.marker().is_file()
			&& self.marker_time() > SystemTime::now() - STALE + Duration::from_secs(60);
		let since = recent.then(|| self.age_marker(RECENT));
		let run = self.run_once(bash, url, harness);
		if let Some(since) = since.filter(|_| url == self.url()) {
			self.wait_for_fetch(since);
		}
		run
	}

	/// Expires the last fetch and runs the loader, which then fetches before serving.
	fn run_updated(&self, bash: &Path, harness: Option<&str>) -> Run {
		self.age_marker(EXPIRED);
		self.run(bash, &self.url(), harness)
	}

	/// Asserts that the cache is a complete checkout of the remote's `main`.
	fn assert_cache_matches_remote(&self) {
		let cache = self.cache();
		assert!(cache.join("GUIDANCE.md").is_file());
		assert_eq!(
			git(&cache, &["rev-parse", "HEAD"]),
			git(&self.remote, &["rev-parse", "main"])
		);
		assert_eq!(git(&cache, &["status", "--porcelain"]), "");
	}

	/// Asserts that no temporary clone and no git lock file was left behind.
	fn assert_clean(&self) {
		assert_eq!(
			entries(&self.root),
			[".agent-guidance", ".agent-guidance-loader"]
		);
		let mut pending = vec![self.cache()];
		while let Some(dir) = pending.pop() {
			for entry in fs::read_dir(dir).expect("reading the cache") {
				let path = entry.expect("reading a cache entry").path();
				let name = path.file_name().expect("a name").to_string_lossy();
				assert!(
					!name.ends_with(".lock") && !name.starts_with(".agent-guidance.tmp"),
					"left {}",
					path.display()
				);
				if path.is_dir() {
					pending.push(path);
				}
			}
		}
	}
}

/// The result of one loader run.
struct Run {
	output: Output,
	elapsed: Duration,
}

impl Run {
	/// Checks the output for `harness` and returns the injected context.
	///
	/// The loader must exit 0 and print exactly one JSON object on one line, with exactly the keys
	/// that harness documents.
	fn context(&self, harness: Option<&str>) -> String {
		let stderr = String::from_utf8_lossy(&self.output.stderr);
		assert!(
			self.output.status.success(),
			"exit status {}: {stderr}",
			self.output.status
		);
		let stdout = std::str::from_utf8(&self.output.stdout).expect("stdout is UTF-8");
		assert!(stdout.ends_with('\n'), "stdout: {stdout:?}");
		assert_eq!(stdout.lines().count(), 1, "stdout: {stdout:?}");
		let output: Value = serde_json::from_str(stdout).expect("stdout is one JSON object");
		let object = output.as_object().expect("stdout is a JSON object");
		let keys: Vec<&str> = object.keys().map(String::as_str).collect();
		let context = match harness {
			Some("cursor") => {
				assert_eq!(keys, ["additional_context"]);
				&object["additional_context"]
			}
			Some("copilot") => {
				assert_eq!(keys, ["additionalContext"]);
				&object["additionalContext"]
			}
			_ => {
				assert_eq!(keys, ["hookSpecificOutput", "systemMessage"]);
				assert!(object["systemMessage"].is_string(), "stdout: {stdout:?}");
				let inner = object["hookSpecificOutput"].as_object().expect("an object");
				let inner_keys: Vec<&str> = inner.keys().map(String::as_str).collect();
				assert_eq!(inner_keys, ["hookEventName", "additionalContext"]);
				assert_eq!(inner["hookEventName"], "SessionStart");
				&inner["additionalContext"]
			}
		};
		context
			.as_str()
			.expect("the context is a string")
			.to_owned()
	}

	/// Checks the output for a harness with the Claude Code shape and returns the status line
	/// shown to the user.
	fn system_message(&self) -> String {
		self.context(None);
		let output: Value = serde_json::from_slice(&self.output.stdout).expect("one JSON object");
		output["systemMessage"]
			.as_str()
			.expect("the status line is a string")
			.to_owned()
	}
}

fn fresh_clone(bash: &Path) {
	let guidance = format!("# Guidance\n\n{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	let first = fixture.run(bash, &fixture.url(), Some("claude"));
	assert_eq!(first.context(Some("claude")), fixture.context(&guidance));
	assert!(first.elapsed < WATCHDOG, "took {:?}", first.elapsed);
	assert!(fixture.marker().is_file());
	fixture.assert_cache_matches_remote();
	let head = fixture.short_head();
	assert_eq!(
		first.system_message(),
		format!("parity-agent-guidance: loaded {head} (cloned)")
	);
	for harness in [None, Some("unknown")]
		.into_iter()
		.chain(HARNESSES.map(Some))
	{
		let run = fixture.run(bash, &fixture.url(), harness);
		assert_eq!(
			run.context(harness),
			fixture.context(&guidance),
			"{harness:?}"
		);
		// Cursor and Copilot document no field that reaches the user; context() checks that
		// their output has no systemMessage.
		if !matches!(harness, Some("cursor" | "copilot")) {
			assert_eq!(
				run.system_message(),
				format!("parity-agent-guidance: loaded {head} (cached)"),
				"{harness:?}"
			);
		}
	}
	fixture.assert_clean();
}

fn background_update(bash: &Path) {
	background_update_with_marker_age(bash, RECENT);
}

fn six_day_marker_does_not_block(bash: &Path) {
	background_update_with_marker_age(bash, DAY.saturating_mul(6));
}

/// With a last-fetch marker `age` old, a run serves the cache and the next run the update.
fn background_update_with_marker_age(bash: &Path, age: Duration) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.publish(&[("GUIDANCE.md", UPDATED.as_bytes())]);

	// The first run serves the cached copy and fetches in the background.
	let since = fixture.age_marker(age);
	let run = fixture.run_once(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert!(run.elapsed < BACKGROUND_RUN, "took {:?}", run.elapsed);
	fixture.wait_for_fetch(since);
	// The fetch did not touch the files.
	assert_eq!(
		fs::read_to_string(fixture.cache().join("GUIDANCE.md")).expect("reading the cache"),
		guidance
	);

	// The next run checks out the fetched commit.
	let run = fixture.run(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn background_fetch_does_not_block(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.age_marker(RECENT);
	let server = StalledServer::start();
	// output() returns when stdout and stderr reach EOF, so the elapsed time also shows that the
	// background fetch does not hold the pipes.
	let run = fixture.run_once(bash, &server.url, None);
	assert!(run.elapsed < BACKGROUND_RUN, "took {:?}", run.elapsed);
	assert_eq!(run.context(None), fixture.context(&guidance));
	server.wait_for_clients_to_leave();
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn background_fetch_survives_group_kill(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.publish(&[("GUIDANCE.md", UPDATED.as_bytes())]);
	let since = fixture.age_marker(RECENT);

	// The slow URL keeps the background fetch running when the process group is killed.
	let child = fixture
		.command(bash, &fixture.slow_url(), None)
		.process_group(0)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.expect("starting the loader");
	let group = format!("-{}", child.id());
	let output = child.wait_with_output().expect("waiting for the loader");
	// The group may already be empty, in which case kill fails; only the fetch matters.
	Command::new("kill")
		.args(["-KILL", "--", &group])
		.stderr(Stdio::null())
		.status()
		.expect("running kill");
	let run = Run {
		output,
		elapsed: Duration::ZERO,
	};
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert!(
		fixture.marker_time() <= since,
		"the fetch finished before the kill"
	);

	fixture.wait_for_fetch(since);
	let run = fixture.run(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	fixture.assert_clean();
}

fn expired_marker_blocks(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.publish(&[("GUIDANCE.md", UPDATED.as_bytes())]);
	let expired = fixture.age_marker(EXPIRED);
	let run = fixture.run_once(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	assert_eq!(
		run.system_message(),
		format!(
			"parity-agent-guidance: loaded {} (fetched)",
			fixture.short_head()
		)
	);
	assert!(fixture.marker_time() > expired + EXPIRED - Duration::from_secs(60));
	assert!(
		!fixture.cache().join(".git/FETCH_HEAD").exists(),
		"FETCH_HEAD is removed once applied"
	);
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();

	// Without a marker, for example in a cache made by an older loader, the run also blocks.
	fs::remove_file(fixture.marker()).expect("removing the marker");
	fixture.publish(&[("GUIDANCE.md", guidance.as_bytes())]);
	let run = fixture.run_once(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert!(fixture.marker().is_file());
	fixture.assert_clean();
}

fn offline_with_cache(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	let head = fixture.short_head();
	// A recent fetch serves the cache and leaves the failing fetch to the background, which the
	// status line cannot know about. An expired one fetches first and reports the failure.
	for (age, status) in [
		(RECENT, format!("loaded {head} (cached)")),
		(EXPIRED, format!("fetch failed, loaded cached {head}")),
	] {
		fixture.age_marker(age);
		let run = fixture.run_once(bash, OFFLINE, None);
		assert_eq!(run.context(None), fixture.context(&guidance));
		assert_eq!(
			run.system_message(),
			format!("parity-agent-guidance: {status}")
		);
	}
	fixture.assert_cache_matches_remote();
}

fn offline_without_cache(bash: &Path) {
	let fixture = Fixture::new(format!("{MARKER}\n").as_bytes());
	for harness in [None, Some("cursor"), Some("copilot")] {
		let run = fixture.run(bash, OFFLINE, harness);
		assert_eq!(run.context(harness), FALLBACK);
		if harness.is_none() {
			assert_eq!(
				run.system_message(),
				format!("parity-agent-guidance: NOT loaded, could not clone {OFFLINE}")
			);
		}
	}
	assert_eq!(entries(&fixture.root), [".agent-guidance-loader"]);
}

fn hung_network_without_cache(bash: &Path) {
	let fixture = Fixture::new(format!("{MARKER}\n").as_bytes());
	let server = StalledServer::start();
	let run = fixture.run_once(bash, &server.url, None);
	assert!(run.elapsed < HUNG_BUDGET, "took {:?}", run.elapsed);
	assert_eq!(run.context(None), FALLBACK);
	assert_eq!(
		run.system_message(),
		format!(
			"parity-agent-guidance: NOT loaded, could not clone {}",
			server.url
		)
	);
	assert_eq!(entries(&fixture.root), [".agent-guidance-loader"]);
}

fn hung_network_with_expired_marker(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.age_marker(EXPIRED);
	let server = StalledServer::start();
	let run = fixture.run_once(bash, &server.url, None);
	assert!(run.elapsed < HUNG_BUDGET, "took {:?}", run.elapsed);
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert_eq!(
		run.system_message(),
		format!(
			"parity-agent-guidance: fetch failed, loaded cached {}",
			fixture.short_head()
		)
	);
	server.wait_for_clients_to_leave();
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn killed_during_clone(bash: &Path) {
	let fixture = Fixture::new(format!("{MARKER}\n").as_bytes());
	let server = StalledServer::start();
	let child = fixture
		.command(bash, &server.url, None)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.expect("starting the loader");
	let start = Instant::now();
	wait_until(WATCHDOG, "the clone connecting", || {
		server.accepted.load(Ordering::SeqCst) > 0
	});
	assert_eq!(entries(&fixture.root).len(), 2, "no temporary clone");
	let status = Command::new("kill")
		.args(["-TERM", &child.id().to_string()])
		.status()
		.expect("running kill");
	assert!(status.success());
	let output = child.wait_with_output().expect("waiting for the loader");
	assert!(output.status.success());
	assert!(start.elapsed() < WATCHDOG, "took {:?}", start.elapsed());
	server.wait_for_clients_to_leave();
	assert_eq!(entries(&fixture.root), [".agent-guidance-loader"]);
}

fn escaping_without_python(bash: &Path) {
	let guidance = "quote \" backslash \\ \\\" tab \t CR \r\n\
		controls \0 \x01 \x07 \x08 \x0b \x0c \x1b \x1f \x7f end\n\
		non-ASCII \u{e4} \u{65e5}\u{672c} \u{1f600} \u{85} \u{2028} \u{2029}\n\
		\n\
		marker with % and %s and `ticks` and $HOME\n\n";
	let fixture = Fixture::new(guidance.as_bytes());

	// Control characters other than tab, LF and CR are removed, and one trailing newline is
	// dropped.
	let kept: String = guidance
		.chars()
		.filter(|c| !c.is_ascii_control() || matches!(c, '\t' | '\n' | '\r'))
		.collect();
	let context = fixture.context(&kept);
	let expected = |context: &str, status: &str| {
		format!(
			"{{\"hookSpecificOutput\":{{\"hookEventName\":\"SessionStart\",\"additionalContext\":{}}},\
			 \"systemMessage\":{}}}\n",
			serde_json::to_string(context).expect("serializing a string"),
			serde_json::to_string(&format!("parity-agent-guidance: {status}"))
				.expect("serializing a string")
		)
	};

	// A python3 wrapper first on PATH records that the loader ran python.
	let python = which("python3").expect("this test needs python3 on PATH");
	let wrapper = tempdir();
	let log = wrapper.path().join("python3.log");
	let script = format!(
		"#!/bin/sh\necho \"$*\" >>'{}'\nexec '{}' \"$@\"\n",
		log.display(),
		python.display()
	);
	fs::write(wrapper.path().join("python3"), script).expect("writing the python3 wrapper");
	fs::set_permissions(
		wrapper.path().join("python3"),
		fs::Permissions::from_mode(0o755),
	)
	.expect("setting the mode");
	let path = env::join_paths(
		[wrapper.path().to_owned()]
			.into_iter()
			.chain(env::split_paths(&env::var_os("PATH").expect("PATH is set"))),
	)
	.expect("joining PATH");
	let start = Instant::now();
	let output = fixture
		.command(bash, &fixture.url(), None)
		.env("PATH", &path)
		.output()
		.expect("running the loader");
	let with_python = Run {
		output,
		elapsed: start.elapsed(),
	};
	assert_eq!(with_python.context(None), context);
	let calls = fs::read_to_string(&log).expect("the loader ran python3");
	assert!(calls.starts_with("-I -c "), "{calls}");
	let head = fixture.short_head();
	assert_eq!(
		String::from_utf8_lossy(&with_python.output.stdout),
		expected(&context, &format!("loaded {head} (cloned)"))
	);

	// A PATH that holds only the tools the loader needs, so the awk escaper runs. The marker is
	// expired so the run also exercises the blocking fetch.
	let bin = tool_dir(true);
	let expired = fixture.age_marker(EXPIRED);
	let mut command = fixture.command(bash, &fixture.url(), None);
	let output = command
		.env("PATH", bin.path())
		.output()
		.expect("running the loader");
	assert_eq!(String::from_utf8_lossy(&output.stderr), "");
	assert_eq!(
		String::from_utf8_lossy(&output.stdout),
		expected(&context, &format!("loaded {head} (fetched)"))
	);
	assert!(output.status.success());
	assert!(fixture.marker_time() > expired);

	// A failed clone puts the URL in the status line. Both escapers give it the same bytes.
	let url = "file:///nonexistent/quote \" backslash \\ tab \t \u{e4}.git";
	let failed = Fixture::new(guidance.as_bytes());
	let outputs = [path.as_os_str(), bin.path().as_os_str()].map(|path| {
		failed
			.command(bash, url, None)
			.env("PATH", path)
			.output()
			.expect("running the loader")
	});
	let status = format!("NOT loaded, could not clone {url}");
	for output in &outputs {
		assert!(output.status.success());
		assert_eq!(
			String::from_utf8_lossy(&output.stdout),
			expected(FALLBACK, &status)
		);
	}
	assert_eq!(outputs[0].stdout, outputs[1].stdout);
}

/// A directory with links to the tools the loader needs, to use as the whole `PATH`. git is
/// included only when `with_git` is set.
fn tool_dir(with_git: bool) -> TempDir {
	let bin = tempdir();
	let tools = [
		"awk", "tr", "cat", "dirname", "rm", "mv", "sleep", "uname", "find", "touch",
	];
	let git = with_git.then_some("git");
	for tool in tools.into_iter().chain(git) {
		let target = which(tool).unwrap_or_else(|| panic!("{tool} on PATH"));
		std::os::unix::fs::symlink(target, bin.path().join(tool)).expect("linking a tool");
	}
	bin
}

fn git_not_found(bash: &Path) {
	let fixture = Fixture::new(format!("{MARKER}\n").as_bytes());
	// One PATH with a python3 that records its calls, so the python escaper runs, and one without,
	// so the awk escaper runs. Neither holds git.
	let python = which("python3").expect("this test needs python3 on PATH");
	let with_python = tool_dir(false);
	let log = with_python.path().join("python3.log");
	let script = format!(
		"#!/bin/sh\necho \"$*\" >>'{}'\nPATH='{}' exec '{}' \"$@\"\n",
		log.display(),
		env::var("PATH").expect("PATH is UTF-8"),
		python.display()
	);
	fs::write(with_python.path().join("python3"), script).expect("writing the python3 wrapper");
	fs::set_permissions(
		with_python.path().join("python3"),
		fs::Permissions::from_mode(0o755),
	)
	.expect("setting the mode");
	let without_python = tool_dir(false);
	let outputs = [with_python.path(), without_python.path()].map(|path| {
		fixture
			.command(bash, &fixture.url(), None)
			.env("PATH", path)
			.output()
			.expect("running the loader")
	});
	assert!(fs::read_to_string(&log).is_ok_and(|calls| calls.starts_with("-I -c ")));
	for output in outputs {
		let run = Run {
			output,
			elapsed: Duration::ZERO,
		};
		assert_eq!(run.context(None), FALLBACK);
		assert_eq!(
			run.system_message(),
			"parity-agent-guidance: NOT loaded, git not found"
		);
	}
	assert_eq!(entries(&fixture.root), [".agent-guidance-loader"]);
}

fn credentials_are_not_shown(bash: &Path) {
	let fixture = Fixture::new(format!("{MARKER}\n").as_bytes());
	// No clone reaches a server: protocol.ssh.allow=never makes git refuse the scp form before it
	// starts ssh, and port 1 refuses the HTTP connections. protocol.ext.allow stays as the fixture
	// sets it.
	for (url, shown) in [
		(
			"user:SECRET@nonexistent.invalid:org/repo.git",
			"nonexistent.invalid:org/repo.git",
		),
		(
			"git@nonexistent.invalid:org/agent@guidance.git",
			"nonexistent.invalid:org/agent@guidance.git",
		),
		(
			"http://user:SECRET@127.0.0.1:1/agent@guidance.git",
			"http://127.0.0.1:1/agent@guidance.git",
		),
		(
			"http://SECRET@127.0.0.1:1/x.git",
			"http://127.0.0.1:1/x.git",
		),
		("http://127.0.0.1:1/x.git", "http://127.0.0.1:1/x.git"),
	] {
		let output = fixture
			.command(bash, url, None)
			.env("GIT_CONFIG_COUNT", "2")
			.env("GIT_CONFIG_KEY_1", "protocol.ssh.allow")
			.env("GIT_CONFIG_VALUE_1", "never")
			.output()
			.expect("running the loader");
		let stderr = String::from_utf8_lossy(&output.stderr);
		if !url.starts_with("http") {
			assert!(stderr.contains("transport 'ssh' not allowed"), "{stderr}");
		}
		let run = Run {
			output,
			elapsed: Duration::ZERO,
		};
		assert_eq!(run.context(None), FALLBACK);
		let status = run.system_message();
		assert_eq!(
			status,
			format!("parity-agent-guidance: NOT loaded, could not clone {shown}")
		);
		assert!(!status.contains("SECRET"), "{status}");
	}
}

fn cache_without_guidance(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fs::remove_file(fixture.work.join("GUIDANCE.md")).expect("removing GUIDANCE.md");
	fixture.push();

	// The fetched commit has no GUIDANCE.md, so nothing is served.
	let run = fixture.run_updated(bash, None);
	assert_eq!(run.context(None), FALLBACK);
	assert_eq!(
		run.system_message(),
		"parity-agent-guidance: NOT loaded, no usable copy in .agent-guidance/"
	);
	assert_eq!(
		git(&fixture.cache(), &["rev-parse", "HEAD"]),
		git(&fixture.remote, &["rev-parse", "main"])
	);

	// A failed fetch is named as well.
	fixture.age_marker(EXPIRED);
	let run = fixture.run_once(bash, OFFLINE, None);
	assert_eq!(run.context(None), FALLBACK);
	assert_eq!(
		run.system_message(),
		"parity-agent-guidance: NOT loaded, fetch failed, no usable copy in .agent-guidance/"
	);
}

fn concurrent_first_runs(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	let children: Vec<_> = (0..2)
		.map(|_| {
			fixture
				.command(bash, &fixture.url(), None)
				.stdout(Stdio::piped())
				.stderr(Stdio::piped())
				.spawn()
				.expect("starting the loader")
		})
		.collect();
	for child in children {
		let output = child.wait_with_output().expect("waiting for the loader");
		let run = Run {
			output,
			elapsed: Duration::ZERO,
		};
		assert_eq!(run.context(None), fixture.context(&guidance));
	}
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn committed_guidance_size(bash: &Path) {
	let guidance = fs::read_to_string(format!("{REPO}/GUIDANCE.md")).expect("reading GUIDANCE.md");
	let fixture = Fixture::new(guidance.as_bytes());
	let context = fixture.run(bash, &fixture.url(), None).context(None);
	assert_eq!(context, fixture.context(&guidance));
	assert!(
		context
			.lines()
			.any(|line| line.starts_with("Verification marker: "))
	);
	let length = context.chars().count();
	assert!(
		length <= 8000,
		"the injected context has {length} characters"
	);
}

fn cache_without_git_in_downstream_repository(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	git(&fixture.root, &["init", "-q", "-b", "main"]);
	fs::create_dir(fixture.cache()).expect("creating the cache directory");
	fs::write(fixture.cache().join("GUIDANCE.md"), "copied by hand\n").expect("writing");
	let assert_downstream_untouched = || {
		assert_eq!(
			git(&fixture.root, &["rev-parse", "--is-shallow-repository"]),
			"false\n"
		);
		assert!(!fixture.root.join(".git/FETCH_HEAD").exists());
		assert!(!fixture.root.join(".git/agent-guidance-last-fetch").exists());
	};

	// Without a clone the directory is neither served nor removed.
	let run = fixture.run_once(bash, OFFLINE, None);
	assert_eq!(run.context(None), FALLBACK);
	assert_eq!(
		fs::read_to_string(fixture.cache().join("GUIDANCE.md")).expect("reading"),
		"copied by hand\n"
	);
	assert_downstream_untouched();

	// A successful clone replaces it.
	let run = fixture.run_once(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert_downstream_untouched();
	fixture.assert_cache_matches_remote();
	assert_eq!(
		entries(&fixture.root),
		[".agent-guidance", ".agent-guidance-loader", ".git"]
	);
}

fn empty_git_directory_heals(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	git(&fixture.root, &["init", "-q", "-b", "main"]);
	fs::create_dir_all(fixture.cache().join(".git")).expect("creating an empty .git");
	fs::write(fixture.cache().join("GUIDANCE.md"), "stale\n").expect("writing");

	// Offline, the broken cache is neither served nor removed.
	let run = fixture.run_once(bash, OFFLINE, None);
	assert_eq!(run.context(None), FALLBACK);
	assert!(fixture.cache().join("GUIDANCE.md").is_file());
	assert_eq!(
		git(&fixture.root, &["rev-parse", "--is-shallow-repository"]),
		"false\n"
	);
	assert!(!fixture.root.join(".git/FETCH_HEAD").exists());

	// The next online run replaces it with a clone.
	let run = fixture.run_once(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert!(!fixture.root.join(".git/FETCH_HEAD").exists());
	fixture.assert_cache_matches_remote();
}

fn zero_background_budget_is_ignored(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.publish(&[("GUIDANCE.md", UPDATED.as_bytes())]);
	let since = fixture.age_marker(RECENT);
	// The slow URL makes the fetch take a second, which a zero budget would cut short.
	let output = fixture
		.command(bash, &fixture.slow_url(), None)
		.env("AGENT_GUIDANCE_BACKGROUND_BUDGET", "0")
		.output()
		.expect("running the loader");
	assert!(output.status.success());
	fixture.wait_for_fetch(since);
	let run = fixture.run(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	fixture.assert_clean();
}

fn ignores_python_modules_in_the_current_directory(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	let cwd = tempdir();
	let imported = cwd.path().join("imported");
	let module = format!(
		"open('{}', 'w').close()\nimport sys\nsys.stdout.write('SHADOWED')\nsys.exit(0)\n",
		imported.display()
	);
	fs::write(cwd.path().join("json.py"), module).expect("writing json.py");
	let output = fixture
		.command(bash, &fixture.url(), None)
		.current_dir(cwd.path())
		.output()
		.expect("running the loader");
	let run = Run {
		output,
		elapsed: Duration::ZERO,
	};
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert!(!imported.exists(), "the loader imported json.py");
	assert_eq!(entries(cwd.path()), ["json.py"]);
}

fn local_edits_are_discarded_on_update(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fs::write(fixture.cache().join("GUIDANCE.md"), "local edit\n").expect("editing the cache");
	fixture.publish(&[("GUIDANCE.md", UPDATED.as_bytes())]);
	let run = fixture.run_updated(bash, None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn unchanged_remote_adds_no_reflog_entries(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	let reflog = || git(&fixture.cache(), &["reflog", "show", "--format=%H", "HEAD"]);
	let before = reflog();
	for _ in 0..3 {
		let run = fixture.run(bash, &fixture.url(), None);
		assert_eq!(run.context(None), fixture.context(&guidance));
		// Every run after the first finds the previous background fetch in FETCH_HEAD.
		assert!(fixture.cache().join(".git/FETCH_HEAD").is_file());
	}
	assert_eq!(reflog(), before);
	fixture.assert_clean();
}

fn force_pushed_remote_updates_the_cache(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	// Replace the remote's history with an unrelated commit.
	git(&fixture.work, &["checkout", "-q", "--orphan", "rewritten"]);
	fs::write(fixture.work.join("GUIDANCE.md"), UPDATED).expect("writing");
	git(&fixture.work, &["add", "-A"]);
	git(&fixture.work, &["commit", "-q", "-m", "rewritten"]);
	let remote = fixture.remote.to_str().expect("UTF-8 path");
	git(
		&fixture.work,
		&["push", "-q", "-f", remote, "rewritten:main"],
	);
	let run = fixture.run_updated(bash, None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn relative_invocation_from_the_root(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	// Cursor runs `.agent-guidance-loader/load.sh`, Copilot `./.agent-guidance-loader/load.sh`,
	// both from the repository root.
	for (script, harness) in [
		(".agent-guidance-loader/load.sh", "cursor"),
		("./.agent-guidance-loader/load.sh", "copilot"),
		(".agent-guidance-loader/load.sh", "cursor"),
	] {
		// Runs after the first one serve the cache and fetch in the background.
		let since = fixture
			.marker()
			.is_file()
			.then(|| fixture.age_marker(RECENT));
		let output = fixture
			.command_running(bash, Path::new(script), &fixture.url(), Some(harness))
			.current_dir(&fixture.root)
			.output()
			.expect("running the loader");
		let run = Run {
			output,
			elapsed: Duration::ZERO,
		};
		assert_eq!(run.context(Some(harness)), fixture.context(&guidance));
		if let Some(since) = since {
			fixture.wait_for_fetch(since);
		}
	}
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

fn background_fetch_times_out(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	fixture.age_marker(RECENT);
	let server = StalledServer::start();
	let start = Instant::now();
	let output = fixture
		.command(bash, &server.url, None)
		.env("AGENT_GUIDANCE_BACKGROUND_BUDGET", "1")
		.output()
		.expect("running the loader");
	assert!(output.status.success());
	// git's own low-speed limit would end the fetch after 3 seconds; the 1 second budget must end
	// it first.
	server.wait_for_clients_to_leave();
	assert!(
		start.elapsed() < Duration::from_millis(2500),
		"the background fetch ran for {:?}",
		start.elapsed()
	);
	assert!(
		fixture.marker_time() < SystemTime::now() - Duration::from_secs(60),
		"a stopped fetch must not touch the marker"
	);
	fixture.assert_clean();
}

fn stale_loader_notice(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	let current = fixture.context(&guidance);
	assert_eq!(
		fixture.run(bash, &fixture.url(), None).context(None),
		current
	);
	let loader: u32 = fs::read_to_string(format!("{REPO}/loader-version"))
		.expect("reading loader-version")
		.trim_end()
		.parse()
		.expect("loader-version holds an integer");
	for (version, notice) in [
		(format!("{}\n", loader + 1), Some(loader + 1)),
		("abc\n".to_owned(), None),
		(format!("{}", loader + 10), Some(loader + 10)),
		(format!("{}x\n", loader + 1), None),
		(String::new(), None),
		("99999999999\n".to_owned(), None),
		("-3\n".to_owned(), None),
		("0\n".to_owned(), None),
		(format!("{loader}\n"), None),
	] {
		fixture.publish(&[("loader-version", version.as_bytes())]);
		let run = fixture.run_updated(bash, None);
		let expected = match notice {
			Some(latest) => format!(
				"{current}\n\nThe agent-guidance loader committed in this repository is version \
				 {loader}; the current version is {latest}. Tell the user to update it by running \
				 agent-guidance-install."
			),
			None => current.clone(),
		};
		assert_eq!(run.context(None), expected, "loader-version {version:?}");
		let status = format!(
			"parity-agent-guidance: loaded {} (fetched){}",
			fixture.short_head(),
			if notice.is_some() { OUT_OF_DATE } else { "" }
		);
		assert_eq!(run.system_message(), status, "loader-version {version:?}");
	}
}

fn unapplied_fetch_is_reported(bash: &Path) {
	let guidance = format!("{MARKER}\n");
	let fixture = Fixture::new(guidance.as_bytes());
	fixture.run(bash, &fixture.url(), None).context(None);
	let head = fixture.short_head();
	fixture.publish(&[("GUIDANCE.md", UPDATED.as_bytes())]);
	// The background fetch of this run stores the update in FETCH_HEAD.
	fixture.run(bash, &fixture.url(), None).context(None);
	// Another session's index lock makes the reset to FETCH_HEAD fail.
	let lock = fixture.cache().join(".git/index.lock");
	fs::write(&lock, "").expect("creating index.lock");
	let run = fixture.run(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(&guidance));
	assert_eq!(
		run.system_message(),
		format!("parity-agent-guidance: could not apply fetch, loaded cached {head}")
	);
	fs::remove_file(&lock).expect("removing index.lock");
	let run = fixture.run(bash, &fixture.url(), None);
	assert_eq!(run.context(None), fixture.context(UPDATED));
	fixture.assert_cache_matches_remote();
	fixture.assert_clean();
}

/// The loader's version equals the `loader-version` file it is compared against.
#[test]
fn loader_version_matches() {
	let loader = fs::read_to_string(LOADER).expect("reading the loader");
	let version = loader
		.lines()
		.find_map(|line| line.strip_prefix("LOADER_VERSION="))
		.expect("the loader sets LOADER_VERSION");
	let file =
		fs::read_to_string(format!("{REPO}/loader-version")).expect("reading loader-version");
	assert_eq!(format!("{version}\n"), file);
}

/// Every index entry in `GUIDANCE.md` names a file that exists in this repository.
#[test]
fn index_entries_exist() {
	let guidance = fs::read_to_string(format!("{REPO}/GUIDANCE.md")).expect("reading GUIDANCE.md");
	let paths: Vec<&str> = guidance
		.lines()
		.filter_map(|line| line.strip_prefix("- `.agent-guidance/"))
		.map(|rest| {
			rest.split('`')
				.next()
				.expect("split yields at least one item")
		})
		.collect();
	assert!(!paths.is_empty());
	for path in paths {
		assert!(
			Path::new(REPO).join(path).is_file(),
			"{path} does not exist"
		);
	}
}
