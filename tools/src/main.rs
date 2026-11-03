//! Installs the agent guidance loader and its hook configs into a repository.
//!
//! Usage: `agent-guidance-install [ROOT]`, where ROOT is the repository root and defaults to the
//! top level of the git repository that contains the current directory. The installer writes
//! `.agent-guidance-loader/load.sh`, adds a SessionStart hook that runs it to the config of each
//! supported harness, ignores the `.agent-guidance/` cache and the loader's temporary clones, and
//! adds a fallback line to `AGENTS.md` and `CLAUDE.md`. Running it again changes nothing unless
//! the templates changed. It never commits; it prints the files to review and add.

use serde_json::{Map, Value};
use std::{
	ffi::OsStr,
	fs, io,
	os::unix::{ffi::OsStrExt, fs::PermissionsExt},
	path::PathBuf,
	process::{Command, ExitCode, Stdio},
};

/// Path of the loader, relative to the repository root.
///
/// Every hook command contains it, so it also identifies an already installed hook entry.
const LOADER_PATH: &str = ".agent-guidance-loader/load.sh";

const LOADER: &str = include_str!("../templates/.agent-guidance-loader/load.sh");

/// Hook configs merged into existing files: path, template contents, and event name.
const HOOK_CONFIGS: [(&str, &str, &str); 4] = [
	(
		".claude/settings.json",
		include_str!("../templates/.claude/settings.json"),
		"SessionStart",
	),
	(
		".codex/hooks.json",
		include_str!("../templates/.codex/hooks.json"),
		"SessionStart",
	),
	(
		".gemini/settings.json",
		include_str!("../templates/.gemini/settings.json"),
		"SessionStart",
	),
	(
		".cursor/hooks.json",
		include_str!("../templates/.cursor/hooks.json"),
		"sessionStart",
	),
];

/// Copilot reads every file in `.github/hooks/`, so this one belongs to the installer alone and
/// is always written from the template.
const COPILOT_CONFIG: (&str, &str) = (
	".github/hooks/agent-guidance.json",
	include_str!("../templates/.github/hooks/agent-guidance.json"),
);

/// Paragraph added to `AGENTS.md` and `CLAUDE.md` for sessions where no hook delivers the
/// guidance.
///
/// It is wrapped at 80 columns. A file that already mentions `agent-guidance`, including one
/// holding the earlier single-line version of this paragraph, is left unchanged.
const FALLBACK: &str = "Company agent guidance lives in `.agent-guidance/GUIDANCE.md`. If that\n\
	directory is missing, clone https://github.com/paritytech/agent-guidance into\n\
	`.agent-guidance/` and read it.";

/// Lines added to `.gitignore`: the cache and the loader's temporary clones.
const GITIGNORE_LINES: [&str; 2] = ["/.agent-guidance/", "/.agent-guidance.tmp.*/"];

const USAGE: &str = "usage: agent-guidance-install [ROOT]

Installs the agent guidance loader and hook configs into the repository at ROOT
(default: the top level of the git repository that contains the current
directory). Run it again to upgrade the loader.";

/// Adds the loader entry of `template` to the `event` hooks of an existing config.
///
/// Returns the new file contents, or `None` when an entry for `event` already runs the loader.
/// All other keys keep their values and order. Top-level keys of the template that the config
/// lacks, such as Cursor's `version`, are added. An error means that the config is not JSON or
/// does not have the expected shape, and the caller must leave the file untouched.
fn merge_hook(existing: &str, template: &str, event: &str) -> Result<Option<String>, String> {
	fn runs_loader(value: &Value) -> bool {
		match value {
			Value::String(s) => s.contains(LOADER_PATH),
			Value::Array(items) => items.iter().any(runs_loader),
			Value::Object(map) => map.values().any(runs_loader),
			_ => false,
		}
	}

	let template: Map<String, Value> =
		serde_json::from_str(template).map_err(|e| format!("invalid template: {e}"))?;
	let entry = template
		.get("hooks")
		.and_then(|hooks| hooks.get(event))
		.and_then(|entries| entries.get(0))
		.ok_or_else(|| format!("template has no `hooks.{event}` entry"))?;

	let mut config: Value =
		serde_json::from_str(existing).map_err(|e| format!("not valid JSON: {e}"))?;
	let Value::Object(config) = &mut config else {
		return Err("the top level is not a JSON object".into());
	};
	for (key, value) in &template {
		if key != "hooks" && !config.contains_key(key) {
			config.insert(key.clone(), value.clone());
		}
	}
	let Value::Object(hooks) = config
		.entry("hooks")
		.or_insert_with(|| Value::Object(Map::new()))
	else {
		return Err("`hooks` is not a JSON object".into());
	};
	let Value::Array(entries) = hooks
		.entry(event)
		.or_insert_with(|| Value::Array(Vec::new()))
	else {
		return Err(format!("`hooks.{event}` is not a JSON array"));
	};
	if entries.iter().any(runs_loader) {
		return Ok(None);
	}
	entries.push(entry.clone());
	let mut text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
	text.push('\n');
	Ok(Some(text))
}

/// Whether the `.gitignore` contents `text` already hold `line` or an equivalent line.
///
/// A line counts as present with or without the leading and trailing slash. Git ignores trailing
/// spaces in a pattern but not leading ones.
fn ignores(text: &str, line: &str) -> bool {
	let pattern = line.trim_matches('/');
	text.lines().any(|existing| {
		let existing = existing.trim_end();
		let existing = existing.strip_prefix('/').unwrap_or(existing);
		existing.strip_suffix('/').unwrap_or(existing) == pattern
	})
}

/// Writes files below the repository root and records which ones changed.
struct Installer {
	root: PathBuf,
	/// Paths written by this run, relative to `root`.
	changed: Vec<&'static str>,
}

impl Installer {
	/// Reads `path`, or returns `None` when it does not exist.
	fn read(&self, path: &str) -> Result<Option<String>, String> {
		match fs::read_to_string(self.root.join(path)) {
			Ok(text) => Ok(Some(text)),
			Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
			Err(e) => Err(format!("reading {path}: {e}")),
		}
	}

	/// Writes `contents` to `path` unless the file already holds exactly these bytes.
	fn write(&mut self, path: &'static str, contents: &str) -> Result<(), String> {
		let full = self.root.join(path);
		if fs::read(&full).is_ok_and(|old| old == contents.as_bytes()) {
			return Ok(());
		}
		if let Some(parent) = full.parent() {
			fs::create_dir_all(parent)
				.map_err(|e| format!("creating {}: {e}", parent.display()))?;
		}
		fs::write(&full, contents).map_err(|e| format!("writing {path}: {e}"))?;
		if !self.changed.contains(&path) {
			self.changed.push(path);
		}
		Ok(())
	}

	/// Appends `line` to `path`, creating the file when it is missing.
	///
	/// A non-empty file is first terminated with a newline. With `paragraph` set, a blank line
	/// separates the new line from the existing text.
	fn append(
		&mut self,
		path: &'static str,
		existing: Option<&str>,
		line: &str,
		paragraph: bool,
	) -> Result<(), String> {
		let mut text = existing.unwrap_or_default().to_owned();
		if !text.is_empty() {
			if !text.ends_with('\n') {
				text.push('\n');
			}
			if paragraph && !text.ends_with("\n\n") {
				text.push('\n');
			}
		}
		text.push_str(line);
		text.push('\n');
		self.write(path, &text)
	}
}

fn main() -> ExitCode {
	let mut args = std::env::args_os().skip(1);
	let root = match (args.next(), args.next()) {
		(None, _) => {
			let toplevel = Command::new("git")
				.args(["rev-parse", "--show-toplevel"])
				.stdin(Stdio::null())
				.stderr(Stdio::null())
				.output();
			match toplevel {
				Ok(output) if output.status.success() => {
					let path = output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout);
					PathBuf::from(OsStr::from_bytes(path))
				}
				Ok(_) => {
					eprintln!(
						"error: the current directory is not in a git repository; \
						 pass the repository root as ROOT"
					);
					return ExitCode::from(2);
				}
				Err(e) => {
					eprintln!("error: running git: {e}; pass the repository root as ROOT");
					return ExitCode::from(2);
				}
			}
		}
		(Some(arg), None) if arg == "-h" || arg == "--help" => {
			println!("{USAGE}");
			return ExitCode::SUCCESS;
		}
		(Some(arg), None) => PathBuf::from(arg),
		(Some(_), Some(_)) => {
			eprintln!("{USAGE}");
			return ExitCode::from(2);
		}
	};
	if !root.is_dir() {
		eprintln!("error: {} is not a directory", root.display());
		return ExitCode::from(2);
	}

	let mut installer = Installer {
		root,
		changed: Vec::new(),
	};
	let mut failed = false;
	let result = (|| -> Result<(), String> {
		installer.write(LOADER_PATH, LOADER)?;
		let loader = installer.root.join(LOADER_PATH);
		let mut permissions = fs::metadata(&loader)
			.map_err(|e| format!("reading {LOADER_PATH}: {e}"))?
			.permissions();
		// git records only the executable bit, so a checkout may have 775 or 755 depending on
		// the umask. Only a missing executable bit is a change.
		if permissions.mode() & 0o111 != 0o111 {
			permissions.set_mode(0o755);
			fs::set_permissions(&loader, permissions)
				.map_err(|e| format!("setting the mode of {LOADER_PATH}: {e}"))?;
			if !installer.changed.contains(&LOADER_PATH) {
				installer.changed.push(LOADER_PATH);
			}
		}

		for (path, template, event) in HOOK_CONFIGS {
			let Some(existing) = installer.read(path)? else {
				installer.write(path, template)?;
				continue;
			};
			match merge_hook(&existing, template, event) {
				Ok(Some(merged)) => installer.write(path, &merged)?,
				Ok(None) => {}
				Err(e) => {
					eprintln!(
						"error: {path}: {e}\n\
						 The file was left unchanged. Add this hook to it by hand:\n{template}"
					);
					failed = true;
				}
			}
		}
		installer.write(COPILOT_CONFIG.0, COPILOT_CONFIG.1)?;

		for line in GITIGNORE_LINES {
			let gitignore = installer.read(".gitignore")?;
			if !gitignore.as_deref().is_some_and(|text| ignores(text, line)) {
				installer.append(".gitignore", gitignore.as_deref(), line, false)?;
			}
		}

		for path in ["AGENTS.md", "CLAUDE.md"] {
			let existing = installer.read(path)?;
			let skip = existing.as_deref().is_some_and(|text| {
				text.contains("agent-guidance")
					|| (path == "CLAUDE.md" && text.contains("@AGENTS.md"))
			});
			if !skip {
				installer.append(path, existing.as_deref(), FALLBACK, true)?;
			}
		}
		Ok(())
	})();
	if let Err(e) = result {
		eprintln!("error: {e}");
		return ExitCode::FAILURE;
	}

	// An installed file that git ignores, for example through a local `.codex/` exclude, is
	// missed by a plain `git add`. Outside a git repository the check fails and is skipped.
	let installed = [
		LOADER_PATH,
		HOOK_CONFIGS[0].0,
		HOOK_CONFIGS[1].0,
		HOOK_CONFIGS[2].0,
		HOOK_CONFIGS[3].0,
		COPILOT_CONFIG.0,
		".gitignore",
		"AGENTS.md",
		"CLAUDE.md",
	];
	let ignored: Vec<String> = Command::new("git")
		.arg("-C")
		.arg(&installer.root)
		.args(["check-ignore", "--"])
		.args(installed)
		.output()
		.map(|output| {
			String::from_utf8_lossy(&output.stdout)
				.lines()
				.map(str::to_owned)
				.collect()
		})
		.unwrap_or_default();
	for path in &ignored {
		eprintln!("warning: git ignores {path}; add it with `git add -f {path}`");
	}

	if installer.changed.is_empty() {
		println!("Nothing to change.");
	} else {
		println!("Changed files:");
		for path in &installer.changed {
			println!("  {path}");
		}
		let (forced, plain): (Vec<&str>, Vec<&str>) = installer
			.changed
			.iter()
			.partition(|path| ignored.iter().any(|i| i == *path));
		println!("Review the changes and add them:");
		if !plain.is_empty() {
			println!("  git add {}", plain.join(" "));
		}
		if !forced.is_empty() {
			println!("  git add -f {}", forced.join(" "));
		}
	}
	if failed {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn parse(text: &str) -> Value {
		serde_json::from_str(text).expect("merge output is JSON")
	}

	#[test]
	fn templates_install_into_an_empty_config() {
		for (path, template, event) in HOOK_CONFIGS {
			let merged = merge_hook("{}", template, event)
				.unwrap_or_else(|e| panic!("{path}: {e}"))
				.unwrap_or_else(|| panic!("{path}: nothing merged"));
			assert_eq!(parse(&merged), parse(template), "{path}");
		}
	}

	#[test]
	fn templates_already_contain_the_loader() {
		for (path, template, event) in HOOK_CONFIGS {
			assert_eq!(merge_hook(template, template, event), Ok(None), "{path}");
		}
	}

	/// This repository installs its own hook and owns its root copies of the installed files.
	///
	/// Checks that, at the repository root, `.agent-guidance-loader/load.sh` and all five hook
	/// configs are byte-identical to their templates, that `load.sh` has all three executable
	/// bits (git records only the executable bit, so the rest of the mode depends on the umask),
	/// and that `.gitignore` holds both ignore lines. A template change that is not copied to
	/// the root fails here.
	#[test]
	fn repository_root_matches_the_templates() {
		let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
		let read = |path: &str| {
			fs::read_to_string(root.join(path))
				.unwrap_or_else(|e| panic!("{path}: {e}; run the installer on this repository"))
		};
		let templates = [(LOADER_PATH, LOADER), COPILOT_CONFIG]
			.into_iter()
			.chain(HOOK_CONFIGS.map(|(path, template, _)| (path, template)));
		for (path, template) in templates {
			assert!(
				read(path) == template,
				"{path} differs from tools/templates/{path}; copy the template over it"
			);
		}
		let mode = fs::metadata(root.join(LOADER_PATH))
			.map(|metadata| metadata.permissions().mode())
			.unwrap_or_else(|e| panic!("{LOADER_PATH}: {e}"));
		assert_eq!(
			mode & 0o111,
			0o111,
			"{LOADER_PATH} must be executable; run the installer on this repository"
		);
		let gitignore = read(".gitignore");
		for line in GITIGNORE_LINES {
			assert!(ignores(&gitignore, line), ".gitignore lacks {line}");
		}
	}

	#[test]
	fn fallback_is_wrapped_at_80_columns() {
		assert!(FALLBACK.lines().count() > 1);
		for line in FALLBACK.lines() {
			assert!(line.chars().count() <= 80, "{line:?}");
		}
		// Detection of an existing fallback relies on this substring.
		assert!(FALLBACK.contains("agent-guidance"));
	}

	/// Without a matcher the hooks run for every session source, `fork` included.
	#[test]
	fn templates_have_no_matcher() {
		for (path, template, event) in HOOK_CONFIGS {
			let entry = &parse(template)["hooks"][event][0];
			assert!(entry.get("matcher").is_none(), "{path}");
		}
	}

	#[test]
	fn copilot_template_runs_the_loader() {
		let config = parse(COPILOT_CONFIG.1);
		assert_eq!(config["version"], 1);
		let command = config["hooks"]["sessionStart"][0]["bash"]
			.as_str()
			.expect("bash command");
		assert!(command.contains(LOADER_PATH));
	}

	#[test]
	fn appends_after_existing_entries_and_keeps_other_keys() {
		let (_, template, event) = HOOK_CONFIGS[0];
		let existing = r#"{
			"permissions": {"allow": ["Bash(ls)"]},
			"hooks": {
				"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "a"}]}],
				"SessionStart": [{"hooks": [{"type": "command", "command": "b"}]}]
			},
			"model": "x"
		}"#;
		let merged = parse(&merge_hook(existing, template, event).unwrap().unwrap());
		let keys: Vec<&String> = merged.as_object().unwrap().keys().collect();
		assert_eq!(keys, ["permissions", "hooks", "model"]);
		let hook_keys: Vec<&String> = merged["hooks"].as_object().unwrap().keys().collect();
		assert_eq!(hook_keys, ["PreToolUse", "SessionStart"]);
		assert_eq!(
			merged["hooks"]["PreToolUse"],
			parse(existing)["hooks"]["PreToolUse"]
		);
		let entries = merged["hooks"]["SessionStart"].as_array().unwrap();
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0]["hooks"][0]["command"], "b");
		assert_eq!(entries[1], parse(template)["hooks"]["SessionStart"][0]);
		assert_eq!(merged["permissions"], parse(existing)["permissions"]);
	}

	#[test]
	fn merging_twice_changes_nothing() {
		for (path, template, event) in HOOK_CONFIGS {
			let existing = r#"{"hooks": {"Other": []}}"#;
			let once = merge_hook(existing, template, event).unwrap().unwrap();
			assert_eq!(merge_hook(&once, template, event), Ok(None), "{path}");
		}
	}

	#[test]
	fn adds_missing_cursor_version_and_keeps_an_existing_one() {
		let (_, template, event) = HOOK_CONFIGS[3];
		let added = parse(
			&merge_hook(r#"{"hooks": {}}"#, template, event)
				.unwrap()
				.unwrap(),
		);
		assert_eq!(added["version"], 1);
		let kept = r#"{"version": 2, "hooks": {"stop": [{"command": "x"}]}}"#;
		let kept = parse(&merge_hook(kept, template, event).unwrap().unwrap());
		assert_eq!(kept["version"], 2);
		assert_eq!(kept["hooks"]["stop"][0]["command"], "x");
	}

	#[test]
	fn rejects_configs_it_cannot_merge() {
		let (_, template, event) = HOOK_CONFIGS[0];
		for existing in [
			"",
			"{",
			"{} trailing",
			"[]",
			r#"{"hooks": []}"#,
			r#"{"hooks": {"SessionStart": {}}}"#,
		] {
			assert!(
				merge_hook(existing, template, event).is_err(),
				"{existing:?}"
			);
		}
	}

	#[test]
	fn recognizes_the_loader_anywhere_in_an_entry() {
		let (_, template, event) = HOOK_CONFIGS[0];
		let existing = r#"{"hooks": {"SessionStart": [{"hooks": [{"type": "command",
			"command": "bash ./.agent-guidance-loader/load.sh claude --extra"}]}]}}"#;
		assert_eq!(merge_hook(existing, template, event), Ok(None));
	}
}
