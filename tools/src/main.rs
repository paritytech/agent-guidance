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

/// Command that updates this installer, and with it the loader it embeds.
const UPDATE_INSTALLER: &str = "cargo install --force --git \
	https://github.com/paritytech/agent-guidance agent-guidance-install";

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
directory). To upgrade the loader, update the installer, then run it again:

  cargo install --force --git https://github.com/paritytech/agent-guidance agent-guidance-install";

/// Reads the version from the `LOADER_VERSION=<digits>` line of a loader, without running it.
///
/// Only the first line that starts with `LOADER_VERSION=` counts. Returns its digits without
/// leading zeros, or `None` when there is no such line or its value is not a decimal integer.
/// The version stays text, so one too large for an integer type still compares as newer with
/// [`is_newer`].
fn loader_version(text: &str) -> Option<&str> {
	let value = text
		.lines()
		.find_map(|line| line.strip_prefix("LOADER_VERSION="))?;
	if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
		return None;
	}
	let digits = value.trim_start_matches('0');
	Some(if digits.is_empty() { "0" } else { digits })
}

/// Whether loader version `a` is greater than `b`, both as returned by [`loader_version`].
fn is_newer(a: &str, b: &str) -> bool {
	(a.len(), a) > (b.len(), b)
}

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

	let embedded = loader_version(LOADER).expect("the embedded loader sets LOADER_VERSION");
	let loader = root.join(LOADER_PATH);
	let existing = match fs::read(&loader) {
		Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
		Err(e) if e.kind() == io::ErrorKind::NotFound => None,
		Err(e) => {
			eprintln!("error: reading {LOADER_PATH}: {e}");
			return ExitCode::FAILURE;
		}
	};
	let existing = existing.as_deref().and_then(loader_version);
	// An installer built before a version bump would downgrade the loader, and its hook configs
	// can be as old as its loader, so it writes nothing.
	if let Some(version) = existing.filter(|version| is_newer(version, embedded)) {
		eprintln!(
			"error: {LOADER_PATH} is loader v{version}, newer than v{embedded} from this \
			 installer.\nNo file was changed. Update the installer and run it again:\n  \
			 {UPDATE_INSTALLER}"
		);
		return ExitCode::FAILURE;
	}
	let loader_report = match existing {
		Some(version) if version == embedded => format!("loader v{embedded} (current)"),
		Some(version) => format!("loader v{version} -> v{embedded}"),
		None => format!("loader ? -> v{embedded}"),
	};

	let mut installer = Installer {
		root,
		changed: Vec::new(),
	};
	let mut failed = false;
	let result = (|| -> Result<(), String> {
		installer.write(LOADER_PATH, LOADER)?;
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

	println!("{loader_report}");
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
mod tests;
