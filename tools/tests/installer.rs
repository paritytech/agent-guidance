//! Tests for the `agent-guidance-install` binary.

mod common;

use common::{git, isolate, tempdir};
use serde_json::Value;
use std::{
	collections::BTreeMap,
	fs,
	os::unix::fs::PermissionsExt,
	path::{Path, PathBuf},
	process::{Command, Output},
};
use tempfile::TempDir;

const INSTALLER: &str = env!("CARGO_BIN_EXE_agent-guidance-install");
const TEMPLATES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/templates");
const GITIGNORE_LINES: &str = "/.agent-guidance/\n/.agent-guidance.tmp.*/\n";
const LOADER: &str = ".agent-guidance-loader/load.sh";
const FALLBACK: &str = "Company agent guidance lives in `.agent-guidance/GUIDANCE.md`. If that\n\
	directory is missing, clone https://github.com/paritytech/agent-guidance into\n\
	`.agent-guidance/` and read it.";

/// The fallback line as earlier installers wrote it, on one line.
const SINGLE_LINE_FALLBACK: &str = "Company agent guidance lives in \
	`.agent-guidance/GUIDANCE.md`. If that directory is missing, clone \
	https://github.com/paritytech/agent-guidance into `.agent-guidance/` and read it.";

/// Hook configs that are merged into existing files, with the event that runs the loader.
const MERGED_CONFIGS: [(&str, &str); 4] = [
	(".claude/settings.json", "SessionStart"),
	(".codex/hooks.json", "SessionStart"),
	(".gemini/settings.json", "SessionStart"),
	(".cursor/hooks.json", "sessionStart"),
];
const COPILOT_CONFIG: &str = ".github/hooks/agent-guidance.json";

/// A downstream git repository.
struct Repo {
	_dir: TempDir,
	root: PathBuf,
}

impl Repo {
	fn new() -> Repo {
		let dir = tempdir();
		let root = dir.path().to_owned();
		git(&root, &["init", "-q", "-b", "main"]);
		Repo { _dir: dir, root }
	}

	fn write(&self, path: &str, contents: &str) {
		let full = self.root.join(path);
		fs::create_dir_all(full.parent().expect("a file has a parent")).expect("creating a dir");
		fs::write(full, contents).expect("writing a file");
	}

	fn read(&self, path: &str) -> String {
		fs::read_to_string(self.root.join(path)).unwrap_or_else(|e| panic!("reading {path}: {e}"))
	}

	fn json(&self, path: &str) -> Value {
		serde_json::from_str(&self.read(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
	}

	/// Runs the installer with the root as its argument.
	fn install(&self) -> Output {
		isolate(Command::new(INSTALLER).arg(&self.root))
			.output()
			.expect("running the installer")
	}

	/// Contents and modes of every file outside `.git`, keyed by relative path.
	fn snapshot(&self) -> BTreeMap<PathBuf, (Vec<u8>, u32)> {
		let mut files = BTreeMap::new();
		let mut pending = vec![self.root.clone()];
		while let Some(dir) = pending.pop() {
			for entry in fs::read_dir(dir).expect("reading a directory") {
				let path = entry.expect("reading a directory entry").path();
				if path.file_name().is_some_and(|name| name == ".git") {
					continue;
				}
				if path.is_dir() {
					pending.push(path);
				} else {
					let mode = fs::metadata(&path)
						.expect("reading metadata")
						.permissions()
						.mode();
					let relative = path
						.strip_prefix(&self.root)
						.expect("below the root")
						.to_owned();
					files.insert(relative, (fs::read(&path).expect("reading a file"), mode));
				}
			}
		}
		files
	}

	fn status(&self) -> String {
		git(
			&self.root,
			&["status", "--porcelain", "--untracked-files=all"],
		)
	}

	/// Asserts that every hook config is valid JSON with exactly one entry running the loader.
	fn assert_hooks_installed(&self) {
		for (path, event) in MERGED_CONFIGS {
			let config = self.json(path);
			let entries = config["hooks"][event]
				.as_array()
				.expect("an array of hook entries");
			let loaders = entries
				.iter()
				.filter(|entry| entry.to_string().contains(LOADER))
				.count();
			assert_eq!(loaders, 1, "{path}");
		}
		assert_eq!(self.read(COPILOT_CONFIG), template(COPILOT_CONFIG));
		assert_eq!(self.json(".cursor/hooks.json")["version"], 1);
		let mode = fs::metadata(self.root.join(LOADER))
			.expect("the loader exists")
			.permissions();
		assert_eq!(mode.mode() & 0o777, 0o755);
		assert_eq!(self.read(LOADER), template(LOADER));
	}

	/// Runs the installer a second time and asserts that nothing changes.
	fn assert_second_run_changes_nothing(&self) {
		let files = self.snapshot();
		let status = self.status();
		let output = self.install();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		assert_eq!(
			String::from_utf8_lossy(&output.stdout),
			format!("{}Nothing to change.\n", current())
		);
		assert_eq!(self.snapshot(), files);
		assert_eq!(self.status(), status);
	}
}

fn template(path: &str) -> String {
	fs::read_to_string(Path::new(TEMPLATES).join(path)).expect("reading a template")
}

/// The `LOADER_VERSION` of the loader template, which the installer embeds.
fn version() -> u32 {
	template(LOADER)
		.lines()
		.find_map(|line| line.strip_prefix("LOADER_VERSION="))
		.expect("the loader sets LOADER_VERSION")
		.parse()
		.expect("LOADER_VERSION is an integer")
}

/// The first line of the installer's output when the loader is already current.
fn current() -> String {
	format!("loader v{} (current)\n", version())
}

/// The loader template with its `LOADER_VERSION` line set to `value`.
fn loader_with_version(value: &str) -> String {
	let template = template(LOADER);
	let loader = template.replacen(
		&format!("\nLOADER_VERSION={}\n", version()),
		&format!("\nLOADER_VERSION={value}\n"),
		1,
	);
	assert_ne!(loader, template);
	loader
}

fn stdout(output: &Output) -> String {
	String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn installs_into_an_empty_repository() {
	let repo = Repo::new();
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(String::from_utf8_lossy(&output.stderr), "");

	for (path, _) in MERGED_CONFIGS {
		assert_eq!(repo.read(path), template(path), "{path}");
	}
	repo.assert_hooks_installed();
	assert_eq!(repo.read(".gitignore"), GITIGNORE_LINES);
	assert_eq!(repo.read("AGENTS.md"), format!("{FALLBACK}\n"));
	assert_eq!(repo.read("CLAUDE.md"), format!("{FALLBACK}\n"));

	let stdout = stdout(&output);
	assert_eq!(
		stdout.lines().next(),
		Some(&*format!("loader ? -> v{}", version()))
	);
	for path in [
		LOADER,
		COPILOT_CONFIG,
		".gitignore",
		"AGENTS.md",
		"CLAUDE.md",
	]
	.into_iter()
	.chain(MERGED_CONFIGS.map(|(path, _)| path))
	{
		assert!(
			stdout.contains(&format!("  {path}\n")),
			"{path} not listed in {stdout}"
		);
	}
	assert!(
		stdout.contains("git add .agent-guidance-loader/load.sh"),
		"{stdout}"
	);

	repo.assert_second_run_changes_nothing();
}

#[test]
fn preserves_existing_content() {
	let repo = Repo::new();
	let agents = "# Project\n\nBuild with make.";
	let claude = "@AGENTS.md\n";
	let settings = r#"{
	"permissions": {"allow": ["Bash(make)"]},
	"hooks": {
		"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "check"}]}],
		"SessionStart": [{"hooks": [{"type": "command", "command": "echo hello"}]}]
	}
}
"#;
	let gitignore = "/target\n*.log";
	repo.write("AGENTS.md", agents);
	repo.write("CLAUDE.md", claude);
	repo.write(".claude/settings.json", settings);
	repo.write(".gitignore", gitignore);
	git(&repo.root, &["add", "-A"]);
	git(&repo.root, &["commit", "-q", "-m", "existing"]);

	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	repo.assert_hooks_installed();

	assert_eq!(repo.read("AGENTS.md"), format!("{agents}\n\n{FALLBACK}\n"));
	assert_eq!(repo.read("CLAUDE.md"), claude);
	assert_eq!(
		repo.read(".gitignore"),
		format!("{gitignore}\n{GITIGNORE_LINES}")
	);
	assert!(!stdout(&output).contains("  CLAUDE.md\n"));

	let before: Value = serde_json::from_str(settings).expect("valid JSON");
	let after = repo.json(".claude/settings.json");
	let keys: Vec<&String> = after.as_object().expect("an object").keys().collect();
	assert_eq!(keys, ["permissions", "hooks"]);
	assert_eq!(after["permissions"], before["permissions"]);
	assert_eq!(after["hooks"]["PreToolUse"], before["hooks"]["PreToolUse"]);
	let session_start = after["hooks"]["SessionStart"].as_array().expect("an array");
	assert_eq!(session_start.len(), 2);
	assert_eq!(session_start[0], before["hooks"]["SessionStart"][0]);

	repo.assert_second_run_changes_nothing();
}

#[test]
fn leaves_an_unparsable_config_untouched() {
	let repo = Repo::new();
	let broken = "{ \"hooks\": { \"SessionStart\": [ }\n";
	repo.write(".codex/hooks.json", broken);

	let output = repo.install();
	assert!(!output.status.success());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("error: .codex/hooks.json: not valid JSON"),
		"{stderr}"
	);
	assert!(stderr.contains(&template(".codex/hooks.json")), "{stderr}");
	assert_eq!(
		fs::read(repo.root.join(".codex/hooks.json")).expect("reading"),
		broken.as_bytes()
	);

	// Everything else is still installed.
	for (path, _) in MERGED_CONFIGS
		.iter()
		.filter(|(path, _)| *path != ".codex/hooks.json")
	{
		assert_eq!(repo.read(path), template(path), "{path}");
	}
	assert_eq!(repo.read("AGENTS.md"), format!("{FALLBACK}\n"));

	// A second run reports the same error and changes nothing else.
	let files = repo.snapshot();
	let output = repo.install();
	assert!(!output.status.success());
	assert_eq!(
		stdout(&output),
		format!("{}Nothing to change.\n", current())
	);
	assert_eq!(repo.snapshot(), files);
}

#[test]
fn recognizes_existing_ignore_lines_and_mentions() {
	// Only the missing ignore line is added.
	let repo = Repo::new();
	repo.write(".gitignore", ".agent-guidance\n");
	assert!(repo.install().status.success());
	assert_eq!(
		repo.read(".gitignore"),
		".agent-guidance\n/.agent-guidance.tmp.*/\n"
	);
	repo.assert_second_run_changes_nothing();

	let repo = Repo::new();
	repo.write(".gitignore", ".agent-guidance.tmp.*\n/.agent-guidance  \n");
	repo.write(
		"AGENTS.md",
		"See https://github.com/paritytech/agent-guidance.\n",
	);
	repo.write("CLAUDE.md", "Guidance: .agent-guidance/GUIDANCE.md\n");
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		repo.read(".gitignore"),
		".agent-guidance.tmp.*\n/.agent-guidance  \n"
	);
	assert_eq!(
		repo.read("AGENTS.md"),
		"See https://github.com/paritytech/agent-guidance.\n"
	);
	assert_eq!(
		repo.read("CLAUDE.md"),
		"Guidance: .agent-guidance/GUIDANCE.md\n"
	);
	repo.assert_second_run_changes_nothing();
}

#[test]
fn keeps_an_earlier_single_line_fallback() {
	let repo = Repo::new();
	let existing = format!("# Project\n\n{SINGLE_LINE_FALLBACK}\n");
	repo.write("AGENTS.md", &existing);
	repo.write("CLAUDE.md", &existing);
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(repo.read("AGENTS.md"), existing);
	assert_eq!(repo.read("CLAUDE.md"), existing);
	repo.assert_second_run_changes_nothing();
}

#[test]
fn warns_about_ignored_files() {
	let repo = Repo::new();
	repo.write(".git/info/exclude", ".codex/\n");
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert_eq!(
		stderr,
		"warning: git ignores .codex/hooks.json; add it with `git add -f .codex/hooks.json`\n"
	);
	assert!(stdout(&output).contains("  git add -f .codex/hooks.json\n"));
}

#[test]
fn upgrades_the_loader_and_restores_its_mode() {
	let repo = Repo::new();
	assert!(repo.install().status.success());
	let loader = repo.root.join(LOADER);
	fs::write(&loader, "#!/bin/sh\nold loader\n").expect("writing the loader");
	fs::set_permissions(&loader, fs::Permissions::from_mode(0o644)).expect("setting the mode");
	let output = repo.install();
	assert!(output.status.success());
	assert_eq!(
		stdout(&output).lines().take(3).collect::<Vec<_>>(),
		[
			&*format!("loader ? -> v{}", version()),
			"Changed files:",
			&format!("  {LOADER}")
		]
	);
	repo.assert_hooks_installed();
	repo.assert_second_run_changes_nothing();
}

#[test]
fn reports_the_version_of_a_replaced_loader() {
	let older = version() - 1;
	for (existing, report) in [
		(
			loader_with_version(&older.to_string()),
			format!("loader v{older} -> v{}", version()),
		),
		(
			loader_with_version("abc"),
			format!("loader ? -> v{}", version()),
		),
	] {
		let repo = Repo::new();
		assert!(repo.install().status.success());
		repo.write(LOADER, &existing);
		let output = repo.install();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		assert_eq!(
			stdout(&output).lines().take(3).collect::<Vec<_>>(),
			[&*report, "Changed files:", &format!("  {LOADER}")]
		);
		repo.assert_hooks_installed();
		repo.assert_second_run_changes_nothing();
	}
}

/// An installer built before a loader version bump writes nothing: its hook configs and other
/// files may be as old as its loader.
#[test]
fn writes_nothing_over_a_newer_loader() {
	// The second version overflows every integer type.
	for newer in [
		(version() + 1).to_string(),
		"99999999999999999999999".to_owned(),
	] {
		let repo = Repo::new();
		let loader = loader_with_version(&newer);
		repo.write(LOADER, &loader);
		fs::set_permissions(repo.root.join(LOADER), fs::Permissions::from_mode(0o755))
			.expect("setting the mode");
		let (path, event) = MERGED_CONFIGS[0];
		let outdated = rewrite(&template_entry(path, event), "timeout", &Value::from(1));
		repo.write(path, &config_with(event, &[outdated]));
		repo.write("AGENTS.md", "# Project\n");
		let files = repo.snapshot();

		let output = repo.install();
		assert_eq!(output.status.code(), Some(1), "v{newer}");
		assert_eq!(
			String::from_utf8_lossy(&output.stderr),
			format!(
				"error: {LOADER} is loader v{newer}, newer than v{} from this installer.\n\
				 No file was changed. Update the installer and run it again:\n  \
				 cargo install --force --git https://github.com/paritytech/agent-guidance \
				 agent-guidance-install\n",
				version()
			)
		);
		assert_eq!(stdout(&output), "", "v{newer}");
		assert_eq!(repo.snapshot(), files, "v{newer}");
	}
}

/// A checkout made with umask 002 has the loader at 775; git sees no change, so neither does the
/// installer.
#[test]
fn keeps_a_group_writable_executable_loader() {
	let repo = Repo::new();
	assert!(repo.install().status.success());
	let loader = repo.root.join(LOADER);
	fs::set_permissions(&loader, fs::Permissions::from_mode(0o775)).expect("setting the mode");
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		stdout(&output),
		format!("{}Nothing to change.\n", current())
	);
	let mode = fs::metadata(&loader)
		.expect("the loader exists")
		.permissions();
	assert_eq!(mode.mode() & 0o777, 0o775);
}

/// A loader with the current content that lost its executable bits is reported and fixed.
#[test]
fn restores_the_mode_of_a_current_loader() {
	let repo = Repo::new();
	assert!(repo.install().status.success());
	let loader = repo.root.join(LOADER);
	fs::set_permissions(&loader, fs::Permissions::from_mode(0o644)).expect("setting the mode");
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		stdout(&output).lines().take(3).collect::<Vec<_>>(),
		[
			current().trim_end(),
			"Changed files:",
			&format!("  {LOADER}")
		]
	);
	assert!(stdout(&output).contains(&format!("git add {LOADER}\n")));
	let mode = fs::metadata(&loader)
		.expect("the loader exists")
		.permissions();
	assert_eq!(mode.mode() & 0o777, 0o755);
	assert_eq!(repo.read(LOADER), template(LOADER));
	repo.assert_second_run_changes_nothing();
}

#[test]
fn defaults_to_the_top_level_of_the_current_repository() {
	let repo = Repo::new();
	let subdirectory = repo.root.join("src/nested");
	fs::create_dir_all(&subdirectory).expect("creating a subdirectory");
	let output = isolate(Command::new(INSTALLER).current_dir(&subdirectory))
		.output()
		.expect("running the installer");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	repo.assert_hooks_installed();
	assert_eq!(
		fs::read_dir(&subdirectory).expect("reading").count(),
		0,
		"wrote into the current directory"
	);
}

#[test]
fn requires_a_root_outside_a_repository() {
	let dir = tempdir();
	let output = isolate(Command::new(INSTALLER).current_dir(dir.path()))
		.output()
		.expect("running the installer");
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("not in a git repository; pass the repository root as ROOT"),
		"{stderr}"
	);
	assert_eq!(fs::read_dir(dir.path()).expect("reading").count(), 0);
}

#[test]
fn rejects_a_missing_root_and_extra_arguments() {
	let repo = Repo::new();
	let missing = repo.root.join("missing");
	let output = isolate(Command::new(INSTALLER).arg(&missing))
		.output()
		.expect("running");
	assert_eq!(output.status.code(), Some(2));
	let output = isolate(Command::new(INSTALLER).args(["a", "b"]))
		.output()
		.expect("running");
	assert_eq!(output.status.code(), Some(2));
	assert_eq!(
		fs::read_dir(&repo.root).expect("reading the root").count(),
		1
	);
}

/// The loader entry of the template at `path` for `event`.
fn template_entry(path: &str, event: &str) -> Value {
	let config: Value = serde_json::from_str(&template(path)).expect("a JSON template");
	config["hooks"][event][0].clone()
}

/// `value` with every value under `key`, at any depth, replaced by `new`.
fn rewrite(value: &Value, key: &str, new: &Value) -> Value {
	match value {
		Value::Array(items) => Value::Array(items.iter().map(|v| rewrite(v, key, new)).collect()),
		Value::Object(map) => Value::Object(
			map.iter()
				.map(|(k, v)| {
					let v = if k == key {
						new.clone()
					} else {
						rewrite(v, key, new)
					};
					(k.clone(), v)
				})
				.collect(),
		),
		other => other.clone(),
	}
}

/// A config with another top-level key and `entries` as the `event` hooks.
fn config_with(event: &str, entries: &[Value]) -> String {
	let config = serde_json::json!({"model": "x", "hooks": {event: entries}});
	let mut text = serde_json::to_string_pretty(&config).expect("serializing");
	text.push('\n');
	text
}

/// An outdated loader entry (another timeout) is replaced where it is, and the user's own
/// entries before and after it are kept.
#[test]
fn updates_an_outdated_loader_entry() {
	let repo = Repo::new();
	let mut expected = Vec::new();
	for (path, event) in MERGED_CONFIGS {
		let entry = template_entry(path, event);
		let before = rewrite(&entry, "command", &Value::from("before"));
		let after = rewrite(&entry, "command", &Value::from("after"));
		let outdated = rewrite(&entry, "timeout", &Value::from(1));
		repo.write(
			path,
			&config_with(event, &[before.clone(), outdated, after.clone()]),
		);
		expected.push(Value::Array(vec![before, entry, after]));
	}
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	for ((path, event), entries) in MERGED_CONFIGS.into_iter().zip(expected) {
		let config = repo.json(path);
		assert_eq!(config["hooks"][event], entries, "{path}");
		assert_eq!(config["model"], "x", "{path}");
		assert!(stdout(&output).contains(&format!("  {path}\n")), "{path}");
	}
	repo.assert_hooks_installed();
	repo.assert_second_run_changes_nothing();
}

#[test]
fn collapses_duplicate_loader_entries() {
	let repo = Repo::new();
	for (path, event) in MERGED_CONFIGS {
		let entry = template_entry(path, event);
		let other = rewrite(&entry, "command", &Value::from("other"));
		let outdated = rewrite(&entry, "timeout", &Value::from(1));
		repo.write(
			path,
			&config_with(event, &[outdated, other.clone(), entry.clone()]),
		);
	}
	let output = repo.install();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	for (path, event) in MERGED_CONFIGS {
		let entry = template_entry(path, event);
		let other = rewrite(&entry, "command", &Value::from("other"));
		assert_eq!(
			repo.json(path)["hooks"][event],
			Value::Array(vec![entry, other]),
			"{path}"
		);
	}
	repo.assert_hooks_installed();
	repo.assert_second_run_changes_nothing();
}

/// `entry` with its loader command followed by `suffix`.
fn chain(entry: &Value, suffix: &str) -> Value {
	let command = entry
		.pointer("/hooks/0/command")
		.or_else(|| entry.get("command"))
		.and_then(Value::as_str)
		.expect("a loader command");
	rewrite(entry, "command", &Value::from(format!("{command}{suffix}")))
}

/// Installs over hook configs whose loader entry `mix` turned into one that runs more than the
/// loader, and asserts that the installer leaves them for the user to edit.
fn assert_mixed_entries_untouched(mix: impl Fn(Value) -> Value) {
	let repo = Repo::new();
	let mut written = Vec::new();
	for (path, event) in MERGED_CONFIGS {
		let config = config_with(event, &[mix(template_entry(path, event))]);
		repo.write(path, &config);
		written.push(config);
	}
	let output = repo.install();
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	for ((path, event), config) in MERGED_CONFIGS.into_iter().zip(&written) {
		assert_eq!(repo.read(path), *config, "{path}");
		assert!(
			stderr.contains(&format!(
				"error: {path}: an entry in `hooks.{event}` runs more than the loader\n"
			)),
			"{stderr}"
		);
		assert!(stderr.contains(&template(path)), "{path}: {stderr}");
	}
	// Everything else is still installed.
	assert_eq!(repo.read(LOADER), template(LOADER));
	assert_eq!(repo.read(COPILOT_CONFIG), template(COPILOT_CONFIG));
	assert_eq!(repo.read("AGENTS.md"), format!("{FALLBACK}\n"));

	let files = repo.snapshot();
	let output = repo.install();
	assert_eq!(output.status.code(), Some(1));
	assert_eq!(
		stdout(&output),
		format!("{}Nothing to change.\n", current())
	);
	assert_eq!(repo.snapshot(), files);
}

/// An entry that runs the loader next to the user's own command is left for the user to edit.
#[test]
fn leaves_a_mixed_loader_entry_untouched() {
	assert_mixed_entries_untouched(|mut entry| {
		match entry.get_mut("hooks").and_then(Value::as_array_mut) {
			Some(hooks) => {
				hooks.push(serde_json::json!({"type": "command", "command": "echo hello"}))
			}
			// A Cursor entry has one command, so here the loader path is in another key.
			None => {
				entry["note"] = entry["command"].clone();
				entry["command"] = Value::from("echo hello");
			}
		}
		entry
	});
}

/// An HTTP hook has no `command`, and replacing its group would delete it.
#[test]
fn leaves_a_loader_group_with_an_http_hook_untouched() {
	assert_mixed_entries_untouched(|mut entry| {
		match entry.get_mut("hooks").and_then(Value::as_array_mut) {
			Some(hooks) => hooks.push(
				serde_json::json!({"type": "http", "url": "https://example.com/session-start"}),
			),
			// A Cursor entry has no group, so here the command chains another one.
			None => entry = chain(&entry, " && notify-me"),
		}
		entry
	});
}

#[test]
fn leaves_a_chained_loader_command_untouched() {
	assert_mixed_entries_untouched(|entry| chain(&entry, " && notify-me"));
}
