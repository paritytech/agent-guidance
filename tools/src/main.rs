//! Installs the agent guidance loader and its hook configs into a repository.
//!
//! Usage: `agent-guidance-install [ROOT]`, where ROOT is the repository root and defaults to the
//! top level of the git repository that contains the current directory. The installer writes
//! `.agent-guidance-loader/load.sh`, adds a SessionStart hook that runs it to the config of each
//! supported harness, ignores the `.agent-guidance/` cache and the loader's temporary clones,
//! re-includes hook configs that git ignores in the local exclude file, puts a fallback paragraph
//! at the top of `AGENTS.md`, and makes `CLAUDE.md` import `AGENTS.md`.
//! Running it again changes nothing unless the templates changed. It never commits; it prints the
//! files to review and add.
//!
//! `agent-guidance-install --user` instead writes the loader to `~/.claude/agent-guidance/load.sh`
//! and adds a SessionStart hook that runs it in user mode to `~/.claude/settings.json`, so Claude
//! Code loads the guidance in every repository. It touches no repository.

use serde_json::{Map, Value};
use std::{
	ffi::{OsStr, OsString},
	fs, io,
	os::unix::{
		ffi::OsStrExt,
		fs::{MetadataExt, PermissionsExt},
	},
	path::{Path, PathBuf},
	process::{Command, ExitCode, Stdio},
};

/// Path of the loader, relative to the repository root.
///
/// Every hook command contains it, so it also identifies an already installed hook entry.
const LOADER_PATH: &str = ".agent-guidance-loader/load.sh";

const LOADER: &str = include_str!("../templates/.agent-guidance-loader/load.sh");

/// Path of the user-level loader, relative to the home directory.
///
/// The user hook command contains it, so it also identifies an already installed user hook entry.
const USER_LOADER_PATH: &str = ".claude/agent-guidance/load.sh";

/// The user-level Claude Code settings, relative to the home directory, and the template merged
/// into them.
const USER_CONFIG: (&str, &str) = (
	".claude/settings.json",
	include_str!("../templates/user/.claude/settings.json"),
);

/// Command that updates this installer, and with it the loader it embeds.
const UPDATE_INSTALLER: &str = "cargo install --git https://github.com/paritytech/agent-guidance";

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

/// Paragraph put at the top of `AGENTS.md` for sessions where no hook delivers the guidance.
///
/// `CLAUDE.md` gets it through [`CLAUDE_IMPORT`]. It is wrapped at 80 columns. A file that
/// already mentions `agent-guidance`, including one holding an earlier version of this paragraph
/// at its end, is left unchanged.
const FALLBACK: &str = "Company agent guidance lives in `.agent-guidance/GUIDANCE.md`. If that\n\
	directory is missing, clone https://github.com/paritytech/agent-guidance into\n\
	`.agent-guidance/` and read it.";

/// Contents of a `CLAUDE.md` written by the installer: Claude Code imports `AGENTS.md` from it.
const CLAUDE_IMPORT: &str = "@AGENTS.md\n";

/// Returns `text` with [`FALLBACK`] as its first paragraph.
///
/// Text that already mentions `agent-guidance` is returned unchanged, and text with nothing but
/// whitespace is replaced. Otherwise the existing bytes follow the paragraph and a blank line.
fn prepend_fallback(text: &str) -> String {
	if text.contains("agent-guidance") {
		text.to_owned()
	} else if text.trim().is_empty() {
		format!("{FALLBACK}\n")
	} else {
		format!("{FALLBACK}\n\n{text}")
	}
}

/// Computes new contents for `AGENTS.md` and `CLAUDE.md` from their current contents, `None`
/// meaning a missing file.
///
/// [`FALLBACK`] goes at the top of `AGENTS.md` (see [`prepend_fallback`]), and `CLAUDE.md`
/// imports `AGENTS.md` with the line [`CLAUDE_IMPORT`].
///
/// - A `CLAUDE.md` that contains `@AGENTS.md` or `@./AGENTS.md` is kept, also when `AGENTS.md` is
///   missing.
/// - Otherwise, when `AGENTS.md` is missing or holds only whitespace, the content of `CLAUDE.md`
///   moves into it.
/// - A missing or empty `CLAUDE.md` is replaced with the import.
/// - When `CLAUDE.md` does not import `AGENTS.md` but either file mentions `agent-guidance`, the
///   repository holds an earlier install and both files are kept.
///
/// Returns the new contents of (`AGENTS.md`, `CLAUDE.md`), `None` where a file stays as it is.
/// An error means that both files hold their own instructions; merging them is left to the
/// maintainer.
fn instruction_files(
	agents: Option<&str>,
	claude: Option<&str>,
) -> Result<(Option<String>, Option<String>), String> {
	let new_agents = |text: &str| {
		let new = prepend_fallback(text);
		(agents != Some(new.as_str())).then_some(new)
	};
	let import = Some(CLAUDE_IMPORT.to_owned());
	let Some(claude) = claude.filter(|text| !text.trim().is_empty()) else {
		return Ok((new_agents(agents.unwrap_or_default()), import));
	};
	if claude.contains("@AGENTS.md") || claude.contains("@./AGENTS.md") {
		return Ok((new_agents(agents.unwrap_or_default()), None));
	}
	let Some(agents) = agents.filter(|text| !text.trim().is_empty()) else {
		return Ok((new_agents(claude), import));
	};
	if agents.contains("agent-guidance") || claude.contains("agent-guidance") {
		return Ok((None, None));
	}
	Err("AGENTS.md and CLAUDE.md both exist and CLAUDE.md does not import AGENTS.md.".into())
}

/// Lines added to `.gitignore`: the cache and the loader's temporary clones.
const GITIGNORE_LINES: [&str; 2] = ["/.agent-guidance/", "/.agent-guidance.tmp.*/"];

const USAGE: &str = "usage: agent-guidance-install [ROOT]
       agent-guidance-install --user

Installs the agent guidance loader and hook configs into the repository at ROOT
(default: the top level of the git repository that contains the current
directory).

With --user, installs the loader into ~/.claude/agent-guidance/ and adds a
SessionStart hook to ~/.claude/settings.json, so Claude Code loads the guidance
in every repository. Repositories with their own loader hook are skipped.

To upgrade the loader, update the installer, then run it again:

  cargo install --git https://github.com/paritytech/agent-guidance";

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

/// Shell text in the Codex template's command that finds the repository root.
const REPOSITORY_ROOT: &str = "$(git rev-parse --show-toplevel)";

/// Whether the shell command `command` runs the loader at `loader` and nothing else.
///
/// It must contain the loader path and no shell syntax that runs a second command: `;`, `&`,
/// `|`, a line break, a backtick, `$(`, `<(` or `>(`. The Codex template's [`REPOSITORY_ROOT`]
/// is allowed.
fn plain_loader_command(command: &str, loader: &str) -> bool {
	let rest = command.replace(REPOSITORY_ROOT, "");
	command.contains(loader)
		&& !rest.contains([';', '&', '|', '\n', '\r', '`'])
		&& !["$(", "<(", ">("]
			.iter()
			.any(|syntax| rest.contains(syntax))
}

/// Whether the hook entry `entry` runs the loader at `loader` and nothing else, so replacing it
/// loses no other hook.
///
/// A Claude Code, Codex or Gemini CLI entry is a group with a `hooks` array. Every hook in it
/// must be a command hook (`type` missing or `"command"`) whose `command` passes
/// [`plain_loader_command`]. A Cursor entry has no group and must be such a hook itself. Other
/// keys of the entry, such as a `matcher` or a `timeout`, do not matter.
fn loader_only(entry: &Value, loader: &str) -> bool {
	let loader_hook = |hook: &Value| {
		hook.get("type").is_none_or(|kind| kind == "command")
			&& hook
				.get("command")
				.and_then(Value::as_str)
				.is_some_and(|command| plain_loader_command(command, loader))
	};
	match entry.get("hooks") {
		Some(Value::Array(hooks)) => !hooks.is_empty() && hooks.iter().all(loader_hook),
		Some(_) => false,
		None => loader_hook(entry),
	}
}

/// Adds or updates the loader entry of `template` in the `event` hooks of an existing config.
///
/// An entry runs the loader when any string in it contains `loader`, the loader's path:
/// [`LOADER_PATH`] for a repository, [`USER_LOADER_PATH`] for the user hook. Entries that run
/// the other loader count as the user's own.
///
/// Returns the new file contents, or `None` when the only entry for `event` that runs the loader
/// equals the template's entry. Without such an entry the template's entry is appended.
/// Otherwise the first entry that runs the loader is replaced in place with the template's
/// entry, and further ones are removed; edits made to them are lost. All other entries and keys
/// keep their values and order. Top-level keys of the template that the config lacks, such as
/// Cursor's `version`, are added.
///
/// An error means that the config is not JSON, does not have the expected shape, or has an
/// entry that runs more than the loader (see [`loader_only`]), which replacing it would delete.
/// The caller must leave the file untouched.
fn merge_hook(
	existing: &str,
	template: &str,
	event: &str,
	loader: &str,
) -> Result<Option<String>, String> {
	fn contains(value: &Value, loader: &str) -> bool {
		match value {
			Value::String(s) => s.contains(loader),
			Value::Array(items) => items.iter().any(|item| contains(item, loader)),
			Value::Object(map) => map.values().any(|item| contains(item, loader)),
			_ => false,
		}
	}
	let runs_loader = |value: &Value| contains(value, loader);

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
	let loaders: Vec<&Value> = entries.iter().filter(|e| runs_loader(e)).collect();
	if let [only] = loaders.as_slice()
		&& *only == entry
	{
		return Ok(None);
	}
	if loaders.iter().any(|e| !loader_only(e, loader)) {
		return Err(format!(
			"an entry in `hooks.{event}` runs more than the loader"
		));
	}
	// Entries before the first loader entry do not run the loader, so it keeps its index.
	let first = entries.iter().position(runs_loader);
	entries.retain(|e| !runs_loader(e));
	entries.insert(first.unwrap_or(entries.len()), entry.clone());
	let mut text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
	text.push('\n');
	Ok(Some(text))
}

/// Paths of all hook configs the installer writes.
const HOOK_CONFIG_PATHS: [&str; 5] = [
	HOOK_CONFIGS[0].0,
	HOOK_CONFIGS[1].0,
	HOOK_CONFIGS[2].0,
	HOOK_CONFIGS[3].0,
	COPILOT_CONFIG.0,
];

/// Directories that contain `path`, from the top down: `.github` and then `.github/hooks` for
/// `.github/hooks/agent-guidance.json`.
fn ancestors(path: &str) -> Vec<&str> {
	let mut dirs: Vec<&str> = Path::new(path)
		.ancestors()
		.skip(1)
		.filter_map(Path::to_str)
		.filter(|dir| !dir.is_empty())
		.collect();
	dirs.reverse();
	dirs
}

/// Exclude lines that make git track the hook config `path` that it ignores.
///
/// Git cannot re-include a file whose directory is excluded. So each ignored directory above
/// the file (`ignored_dirs`, from the top down) is re-included and everything else in it, for
/// example `.codex/worktrees/`, ignored again before the file is re-included.
fn reinclude_lines(path: &str, ignored_dirs: &[&str]) -> Vec<String> {
	let mut lines = Vec::new();
	for dir in ignored_dirs {
		lines.push(format!("!/{dir}/"));
		lines.push(format!("/{dir}/*"));
	}
	lines.push(format!("!/{path}"));
	lines
}

/// Appends each group of lines to the exclude file contents `text` unless the whole group is
/// already there as consecutive lines in the same order. Returns `None` when nothing is added.
///
/// The last matching pattern wins, so a group whose lines are present but out of order may
/// not re-include its file and is added again as a whole.
fn add_exclude_groups(text: &str, groups: &[Vec<String>]) -> Option<String> {
	let mut new = text.to_owned();
	for group in groups {
		let lines: Vec<&str> = new.lines().map(str::trim_end).collect();
		if lines
			.windows(group.len())
			.any(|window| window == group.as_slice())
		{
			continue;
		}
		if !new.is_empty() && !new.ends_with('\n') {
			new.push('\n');
		}
		for line in group {
			new.push_str(line);
			new.push('\n');
		}
	}
	(new != text).then_some(new)
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

/// Files to write, relative to the repository root, with their new contents.
type Plan = Vec<(&'static str, String)>;

/// Writes files below the repository root, or the home directory, and records which ones changed.
struct Installer {
	root: PathBuf,
	/// Paths written by this run, relative to `root`.
	changed: Vec<&'static str>,
	/// Shown before a path relative to `root` in messages: empty for a repository, `~/` for the
	/// home directory.
	prefix: &'static str,
}

impl Installer {
	/// Compares the loader at `loader` with the embedded one and returns the line that reports
	/// the versions.
	///
	/// The version is read from the `LOADER_VERSION` line, without running the file. An error
	/// means that the loader cannot be read, or that it is newer than the embedded one: an
	/// installer built before a version bump would downgrade it, and its hook configs can be as
	/// old as its loader. The caller must then write nothing.
	fn loader_report(&self, loader: &str) -> Result<String, String> {
		let embedded = loader_version(LOADER).expect("the embedded loader sets LOADER_VERSION");
		let shown = format!("{}{loader}", self.prefix);
		let existing = match fs::read(self.root.join(loader)) {
			Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
			Err(e) if e.kind() == io::ErrorKind::NotFound => None,
			Err(e) => return Err(format!("reading {shown}: {e}")),
		};
		Ok(match existing.as_deref().and_then(loader_version) {
			Some(version) if is_newer(version, embedded) => {
				return Err(format!(
					"{shown} is loader v{version}, newer than v{embedded} from this installer.\n\
					 No file was changed. Update the installer and run it again:\n  \
					 {UPDATE_INSTALLER}"
				));
			}
			Some(version) if version == embedded => format!("loader v{embedded} (current)"),
			Some(version) => format!("loader v{version} -> v{embedded}"),
			None => format!("loader ? -> v{embedded}"),
		})
	}

	/// Whether `path` is a symbolic link.
	fn is_symlink(&self, path: &str) -> bool {
		fs::symlink_metadata(self.root.join(path))
			.is_ok_and(|metadata| metadata.file_type().is_symlink())
	}

	/// Prints an error and returns `true` when writing `plan` would write through a symbolic link.
	///
	/// A write through a link would change the link's target, which can be any file, or create
	/// it. Each planned path and every directory above it below the root count. So does the
	/// loader at `loader` when it lacks an executable bit, since [`Self::write_plan`] sets it.
	fn refuse_links(&self, plan: &Plan, loader: &'static str) -> bool {
		let mut written: Vec<&str> = plan.iter().map(|(path, _)| *path).collect();
		if fs::metadata(self.root.join(loader))
			.is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0o111)
			&& !written.contains(&loader)
		{
			written.push(loader);
		}
		let mut links: Vec<&str> = Vec::new();
		for path in written {
			for part in ancestors(path).into_iter().chain([path]) {
				if self.is_symlink(part) && !links.contains(&part) {
					links.push(part);
				}
			}
		}
		if links.is_empty() {
			return false;
		}
		let listed: String = links
			.iter()
			.map(|link| format!("  {}{link}\n", self.prefix))
			.collect();
		eprintln!(
			"error: the installer does not write through symbolic links, and it would write \
			 through these:\n{listed}No file was changed. Replace each link with a regular file \
			 or directory and run the installer again."
		);
		true
	}

	/// Writes `plan`, then gives the loader at `loader` mode 755 when it lacks an executable bit.
	///
	/// git records only the executable bit, so a checkout may have 775 or 755 depending on the
	/// umask. Only a missing executable bit is a change.
	fn write_plan(&mut self, plan: &Plan, loader: &'static str) -> Result<(), String> {
		for (path, contents) in plan {
			self.write(path, contents)?;
		}
		let full = self.root.join(loader);
		let mut permissions = fs::metadata(&full)
			.map_err(|e| format!("reading {}{loader}: {e}", self.prefix))?
			.permissions();
		if permissions.mode() & 0o111 != 0o111 {
			permissions.set_mode(0o755);
			fs::set_permissions(&full, permissions)
				.map_err(|e| format!("setting the mode of {}{loader}: {e}", self.prefix))?;
			if !self.changed.contains(&loader) {
				self.changed.push(loader);
			}
		}
		Ok(())
	}

	/// Drops the files from `plan` that already hold the planned bytes.
	fn drop_unchanged(&self, plan: &mut Plan) {
		plan.retain(|(path, contents)| {
			!fs::read(self.root.join(path)).is_ok_and(|old| old == contents.as_bytes())
		});
	}

	/// Reads `path`, or returns `None` when it does not exist.
	fn read(&self, path: &str) -> Result<Option<String>, String> {
		match fs::read_to_string(self.root.join(path)) {
			Ok(text) => Ok(Some(text)),
			Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
			Err(e) => Err(format!("reading {}{path}: {e}", self.prefix)),
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
		fs::write(&full, contents).map_err(|e| format!("writing {}{path}: {e}", self.prefix))?;
		if !self.changed.contains(&path) {
			self.changed.push(path);
		}
		Ok(())
	}

	/// Returns the paths among `paths` that git ignores, as `git check-ignore` prints them.
	///
	/// Outside a git repository the check fails and nothing is returned.
	fn ignored(&self, paths: &[&str]) -> Vec<String> {
		Command::new("git")
			.arg("-C")
			.arg(&self.root)
			.args(["check-ignore", "--"])
			.args(paths)
			.stdin(Stdio::null())
			.output()
			.map(|output| {
				String::from_utf8_lossy(&output.stdout)
					.lines()
					.map(str::to_owned)
					.collect()
			})
			.unwrap_or_default()
	}

	/// Runs git with `args` in `root` and returns its output without the final line break, or
	/// `None` when git fails, for example outside a git repository.
	///
	/// A path that `rev-parse --git-path` prints is relative to `root` unless it is absolute, as
	/// in a linked worktree.
	fn git(&self, args: &[&str]) -> Option<String> {
		let output = Command::new("git")
			.arg("-C")
			.arg(&self.root)
			.args(args)
			.stdin(Stdio::null())
			.stderr(Stdio::null())
			.output()
			.ok()
			.filter(|output| output.status.success())?;
		let text = String::from_utf8(output.stdout).ok()?;
		Some(text.strip_suffix('\n').unwrap_or(&text).to_owned())
	}
}

/// Appends `line` to the file contents `text`, first terminating non-empty text with a newline.
fn append_line(text: &mut String, line: &str) {
	if !text.is_empty() && !text.ends_with('\n') {
		text.push('\n');
	}
	text.push_str(line);
	text.push('\n');
}

/// The error printed for a hook config at `shown` that `merge_hook` refused with `e`.
fn merge_failure(shown: &str, e: &str, template: &str) -> String {
	format!(
		"error: {shown}: {e}\n\
		 The file was left unchanged. Add or update its loader hook by hand to match:\n{template}"
	)
}

/// Installs the user-level loader and its hook into the home directory, `$HOME`.
///
/// Writes [`USER_LOADER_PATH`] and merges the user hook entry into the settings of
/// [`USER_CONFIG`], with the same version check and merge rules as a repository install. No other
/// file is written and git is not run.
///
/// Unlike a repository install, it writes through symbolic links below `HOME`, such as a
/// `~/.claude` or `~/.claude/settings.json` kept in a dotfiles repository: the link's target is
/// updated and the link stays.
fn install_user() -> ExitCode {
	let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
		eprintln!("error: HOME is not set; --user installs into $HOME/.claude");
		return ExitCode::from(2);
	};
	let home = PathBuf::from(home);
	if !home.is_dir() {
		eprintln!("error: HOME ({}) is not a directory", home.display());
		return ExitCode::from(2);
	}
	let mut installer = Installer {
		root: home,
		changed: Vec::new(),
		prefix: "~/",
	};
	let loader_report = match installer.loader_report(USER_LOADER_PATH) {
		Ok(report) => report,
		Err(e) => {
			eprintln!("error: {e}");
			return ExitCode::FAILURE;
		}
	};
	let (path, template) = USER_CONFIG;
	let mut plan = vec![(USER_LOADER_PATH, LOADER.to_owned())];
	let mut merge_error = None;
	match installer.read(path) {
		Ok(None) => plan.push((path, template.to_owned())),
		Ok(Some(existing)) => {
			match merge_hook(&existing, template, "SessionStart", USER_LOADER_PATH) {
				Ok(Some(merged)) => plan.push((path, merged)),
				Ok(None) => {}
				Err(e) => merge_error = Some(merge_failure(&format!("~/{path}"), &e, template)),
			}
		}
		Err(e) => {
			eprintln!("error: {e}");
			return ExitCode::FAILURE;
		}
	}
	installer.drop_unchanged(&mut plan);
	if let Some(e) = &merge_error {
		eprintln!("{e}");
	}
	if let Err(e) = installer.write_plan(&plan, USER_LOADER_PATH) {
		eprintln!("error: {e}");
		return ExitCode::FAILURE;
	}
	println!("{loader_report}");
	if installer.changed.is_empty() {
		println!("Nothing to change.");
	} else {
		println!("Changed files:");
		for path in &installer.changed {
			println!("  ~/{path}");
		}
	}
	if merge_error.is_some() {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	}
}

fn main() -> ExitCode {
	let args: Vec<OsString> = std::env::args_os().skip(1).collect();
	if args.iter().any(|arg| arg == "--user") {
		if args.len() > 1 {
			eprintln!("error: --user takes no other argument\n\n{USAGE}");
			return ExitCode::from(2);
		}
		return install_user();
	}
	let mut args = args.into_iter();
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
		prefix: "",
	};
	let loader_report = match installer.loader_report(LOADER_PATH) {
		Ok(report) => report,
		Err(e) => {
			eprintln!("error: {e}");
			return ExitCode::FAILURE;
		}
	};
	// A refusal to merge AGENTS.md and CLAUDE.md, like a downgrade, leaves every file unchanged.
	// When one name links to the other, including a dangling link, or both are hard links to one
	// file, CLAUDE.md acts as an import and is never written; only AGENTS.md gets the fallback.
	let file_id = |path| {
		fs::metadata(installer.root.join(path))
			.ok()
			.map(|metadata| (metadata.dev(), metadata.ino()))
	};
	let links_to = |link, target: &str| {
		fs::read_link(installer.root.join(link))
			.is_ok_and(|to| to == Path::new(target) || to == Path::new(".").join(target))
	};
	let agents_links_to_claude = links_to("AGENTS.md", "CLAUDE.md");
	let linked = agents_links_to_claude
		|| links_to("CLAUDE.md", "AGENTS.md")
		|| file_id("AGENTS.md").is_some_and(|agents| file_id("CLAUDE.md") == Some(agents));
	let instructions = installer.read("AGENTS.md").and_then(|agents| {
		let claude = if linked {
			Some(CLAUDE_IMPORT.to_owned())
		} else {
			installer.read("CLAUDE.md")?
		};
		Ok(instruction_files(agents.as_deref(), claude.as_deref()))
	});
	let (agents_md, claude_md) = match instructions {
		Ok(Ok(contents)) => contents,
		Ok(Err(e)) => {
			eprintln!(
				"error: {e}\nNo file was changed. Merge CLAUDE.md into AGENTS.md, replace \
				 CLAUDE.md with the line @AGENTS.md, and run the installer again."
			);
			return ExitCode::FAILURE;
		}
		Err(e) => {
			eprintln!("error: {e}");
			return ExitCode::FAILURE;
		}
	};
	// The new AGENTS.md of an AGENTS.md that links to CLAUDE.md goes into CLAUDE.md, so that the
	// real file is written and listed. CLAUDE.md has no contents of its own then.
	let writes: Vec<(&'static str, String)> = if agents_links_to_claude {
		agents_md
			.map(|text| ("CLAUDE.md", text))
			.into_iter()
			.collect()
	} else {
		[("AGENTS.md", agents_md), ("CLAUDE.md", claude_md)]
			.into_iter()
			.filter_map(|(path, text)| Some((path, text?)))
			.collect()
	};
	// A CLAUDE.md hard-linked to AGENTS.md changes with it, and git sees both names change.
	let hard_linked =
		linked && !installer.is_symlink("AGENTS.md") && !installer.is_symlink("CLAUDE.md");

	// Every file the installer writes is planned first, so that a refusal changes nothing.
	let planned = (|| -> Result<(Plan, Vec<String>), String> {
		let mut plan = vec![(LOADER_PATH, LOADER.to_owned())];
		// Hook configs that cannot be merged are left unchanged and reported.
		let mut merge_errors = Vec::new();
		for (path, template, event) in HOOK_CONFIGS {
			let Some(existing) = installer.read(path)? else {
				plan.push((path, template.to_owned()));
				continue;
			};
			match merge_hook(&existing, template, event, LOADER_PATH) {
				Ok(Some(merged)) => plan.push((path, merged)),
				Ok(None) => {}
				Err(e) => merge_errors.push(merge_failure(path, &e, template)),
			}
		}
		plan.push((COPILOT_CONFIG.0, COPILOT_CONFIG.1.to_owned()));
		let gitignore = installer.read(".gitignore")?;
		let mut text = gitignore.clone().unwrap_or_default();
		for line in GITIGNORE_LINES {
			if !ignores(&text, line) {
				append_line(&mut text, line);
			}
		}
		if gitignore.as_deref() != Some(text.as_str()) {
			plan.push((".gitignore", text));
		}
		plan.extend(writes);
		installer.drop_unchanged(&mut plan);
		Ok((plan, merge_errors))
	})();
	let (plan, merge_errors) = match planned {
		Ok(planned) => planned,
		Err(e) => {
			eprintln!("error: {e}");
			return ExitCode::FAILURE;
		}
	};
	if installer.refuse_links(&plan, LOADER_PATH) {
		return ExitCode::FAILURE;
	}

	for e in &merge_errors {
		eprintln!("{e}");
	}
	let failed = !merge_errors.is_empty();
	// The local exclude file, when this run changed it and so made git track a hook config that
	// it ignored. The file is not committed.
	let mut reincluded = None;
	let result = (|| -> Result<(), String> {
		installer.write_plan(&plan, LOADER_PATH)?;
		if hard_linked
			&& installer.changed.contains(&"AGENTS.md")
			&& !installer.changed.contains(&"CLAUDE.md")
		{
			installer.changed.push("CLAUDE.md");
		}

		// A global excludes file that ignores `.claude/` or `.codex/` would keep a hook config
		// out of a plain `git add`. The lines that re-include it go to the local exclude file:
		// the shared `.gitignore` must not encode one person's global excludes. The exclude file
		// takes precedence over `core.excludesFile` but not over a tracked `.gitignore`, so a
		// rule of the repository itself still wins and the warning below still shows. The lines
		// are anchored at the top level, so below it the step is skipped and only the warning
		// shows.
		let at_top_level = installer
			.git(&["rev-parse", "--show-prefix"])
			.is_some_and(|prefix| prefix.is_empty());
		let ignored_hooks = installer.ignored(&HOOK_CONFIG_PATHS);
		let groups: Vec<Vec<String>> = ignored_hooks
			.iter()
			.map(|path| {
				let dirs: Vec<&str> = ancestors(path)
					.into_iter()
					.filter(|dir| !installer.ignored(&[&format!("{dir}/")]).is_empty())
					.collect();
				reinclude_lines(path, &dirs)
			})
			.collect();
		if let Some(exclude) = installer
			.git(&["rev-parse", "--git-path", "info/exclude"])
			.filter(|exclude| at_top_level && !groups.is_empty() && !exclude.is_empty())
		{
			let full = installer.root.join(&exclude);
			let text = match fs::read_to_string(&full) {
				Ok(text) => text,
				Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
				Err(e) => return Err(format!("reading {exclude}: {e}")),
			};
			if let Some(new) = add_exclude_groups(&text, &groups) {
				if let Some(parent) = full.parent() {
					fs::create_dir_all(parent)
						.map_err(|e| format!("creating {}: {e}", parent.display()))?;
				}
				fs::write(&full, new).map_err(|e| format!("writing {exclude}: {e}"))?;
				// The lines may not help, for example against the repository's `.gitignore`.
				let still_ignored = installer.ignored(&HOOK_CONFIG_PATHS);
				if ignored_hooks
					.iter()
					.any(|path| !still_ignored.contains(path))
				{
					reincluded = Some(exclude);
				}
			}
		}
		Ok(())
	})();
	if let Err(e) = result {
		eprintln!("error: {e}");
		return ExitCode::FAILURE;
	}

	// An installed file that git still ignores, for example through a `.gitignore` in its
	// directory, is missed by a plain `git add`.
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
	let ignored = installer.ignored(&installed);
	for path in &ignored {
		eprintln!("warning: git ignores {path}; add it with `git add -f {path}`");
	}

	println!("{loader_report}");
	if installer.changed.is_empty() && reincluded.is_none() {
		println!("Nothing to change.");
	}
	if let Some(exclude) = &reincluded {
		println!(
			"Re-included ignored hook configs in {exclude}, which is local to this clone and \
			 not committed."
		);
	}
	if !installer.changed.is_empty() {
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
