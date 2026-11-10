use super::*;

fn parse(text: &str) -> Value {
	serde_json::from_str(text).expect("merge output is JSON")
}

/// Every hook config template with its event and the loader path that identifies its entry: the
/// repository configs, then the user settings.
fn configs() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
	HOOK_CONFIGS
		.iter()
		.map(|&(path, template, event)| (path, template, event, LOADER_PATH))
		.chain([(
			"~/.claude/settings.json",
			USER_CONFIG.1,
			"SessionStart",
			USER_LOADER_PATH,
		)])
		.collect()
}

#[test]
fn templates_install_into_an_empty_config() {
	for (path, template, event, loader) in configs() {
		let merged = merge_hook("{}", template, event, loader)
			.unwrap_or_else(|e| panic!("{path}: {e}"))
			.unwrap_or_else(|| panic!("{path}: nothing merged"));
		assert_eq!(parse(&merged), parse(template), "{path}");
	}
}

#[test]
fn templates_already_contain_the_loader() {
	for (path, template, event, loader) in configs() {
		assert_eq!(
			merge_hook(template, template, event, loader),
			Ok(None),
			"{path}"
		);
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
fn reads_the_loader_version_as_text() {
	assert_eq!(
		loader_version("#!/usr/bin/env bash\n# x\nLOADER_VERSION=7\nLOADER_VERSION=8\n"),
		Some("7")
	);
	for (text, version) in [
		("LOADER_VERSION=7\r\n", "7"),
		("LOADER_VERSION=007\n", "7"),
		("LOADER_VERSION=0\n", "0"),
		("LOADER_VERSION=000\n", "0"),
		(
			"LOADER_VERSION=99999999999999999999999\n",
			"99999999999999999999999",
		),
	] {
		assert_eq!(loader_version(text), Some(version), "{text:?}");
	}
	for text in [
		"",
		"#!/bin/sh\nold loader\n",
		"LOADER_VERSION=\n",
		"LOADER_VERSION=abc\n",
		"LOADER_VERSION=+7\n",
		"LOADER_VERSION=-7\n",
		"LOADER_VERSION=7 # comment\n",
		" LOADER_VERSION=7\n",
		"# LOADER_VERSION=7\n",
		"LOADER_VERSION=x\nLOADER_VERSION=7\n",
	] {
		assert_eq!(loader_version(text), None, "{text:?}");
	}
}

#[test]
fn compares_loader_versions_of_any_length() {
	assert!(is_newer("5", "4"));
	assert!(is_newer("10", "9"));
	assert!(is_newer("99999999999999999999999", "4"));
	assert!(!is_newer("4", "4"));
	assert!(!is_newer("9", "10"));
	assert!(!is_newer("0", "4"));
}

#[test]
fn embedded_loader_has_a_version() {
	assert!(loader_version(LOADER).is_some());
}

#[test]
fn usage_names_the_installer_update() {
	assert!(USAGE.contains(UPDATE_INSTALLER), "{USAGE}");
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

#[test]
fn lists_ancestors_from_the_top_down() {
	assert_eq!(
		ancestors(".github/hooks/agent-guidance.json"),
		[".github", ".github/hooks"]
	);
	assert_eq!(ancestors(".claude/settings.json"), [".claude"]);
	assert!(ancestors("AGENTS.md").is_empty());
}

#[test]
fn reincludes_an_ignored_file_and_its_ignored_directories() {
	assert_eq!(
		reinclude_lines(".claude/settings.json", &[".claude"]),
		["!/.claude/", "/.claude/*", "!/.claude/settings.json"]
	);
	assert_eq!(
		reinclude_lines(
			".github/hooks/agent-guidance.json",
			&[".github", ".github/hooks"]
		),
		[
			"!/.github/",
			"/.github/*",
			"!/.github/hooks/",
			"/.github/hooks/*",
			"!/.github/hooks/agent-guidance.json"
		]
	);
	assert_eq!(
		reinclude_lines(".codex/hooks.json", &[]),
		["!/.codex/hooks.json"]
	);
}

#[test]
fn adds_an_exclude_group_unless_it_is_there_in_order() {
	let group = reinclude_lines(".codex/hooks.json", &[".codex"]);
	let groups = [group.clone()];
	let block = format!("{}\n", group.join("\n"));

	assert_eq!(add_exclude_groups("", &groups), Some(block.clone()));
	assert_eq!(
		add_exclude_groups("/.codex/", &groups),
		Some(format!("/.codex/\n{block}"))
	);
	assert_eq!(add_exclude_groups(&block, &groups), None);
	assert_eq!(
		add_exclude_groups(&format!("# local\n{block}/target/\n"), &groups),
		None
	);

	// The same lines out of order or apart do not count.
	let reordered = "!/.codex/hooks.json\n!/.codex/\n/.codex/*\n";
	assert_eq!(
		add_exclude_groups(reordered, &groups),
		Some(format!("{reordered}{block}"))
	);
	let apart = "!/.codex/\n/.codex/*\n/.codex/x\n!/.codex/hooks.json\n";
	assert_eq!(
		add_exclude_groups(apart, &groups),
		Some(format!("{apart}{block}"))
	);
}

/// `FALLBACK` followed by a blank line and `text`.
fn with_fallback(text: &str) -> String {
	format!("{FALLBACK}\n\n{text}")
}

#[test]
fn creates_both_instruction_files() {
	assert_eq!(
		instruction_files(None, None),
		Ok((
			Some(format!("{FALLBACK}\n")),
			Some(CLAUDE_IMPORT.to_owned())
		))
	);
}

#[test]
fn moves_a_lone_claude_md_into_agents_md() {
	let claude = "# Project\n\nRun `make test`.\n";
	assert_eq!(
		instruction_files(None, Some(claude)),
		Ok((Some(with_fallback(claude)), Some(CLAUDE_IMPORT.to_owned())))
	);
}

#[test]
fn moves_a_lone_claude_md_that_mentions_agent_guidance_unchanged() {
	let claude = format!("# Project\n\n{FALLBACK}\n");
	assert_eq!(
		instruction_files(None, Some(&claude)),
		Ok((Some(claude.clone()), Some(CLAUDE_IMPORT.to_owned())))
	);
}

#[test]
fn prepends_to_a_lone_agents_md_and_creates_claude_md() {
	let agents = "# Project\n";
	assert_eq!(
		instruction_files(Some(agents), None),
		Ok((Some(with_fallback(agents)), Some(CLAUDE_IMPORT.to_owned())))
	);
}

#[test]
fn prepends_to_agents_md_when_claude_md_imports_it() {
	let agents = "# Project\n";
	assert_eq!(
		instruction_files(Some(agents), Some(CLAUDE_IMPORT)),
		Ok((Some(with_fallback(agents)), None))
	);
}

#[test]
fn leaves_a_claude_md_with_the_import_and_other_lines_alone() {
	let agents = format!("{FALLBACK}\n\n# Project\n");
	let claude = "@AGENTS.md\n\nUse Opus for reviews.\n";
	assert_eq!(
		instruction_files(Some(&agents), Some(claude)),
		Ok((None, None))
	);
	assert_eq!(
		instruction_files(Some("# Project\n"), Some(claude)),
		Ok((Some(with_fallback("# Project\n")), None))
	);
}

/// The earlier installer appended the fallback to both files instead of importing `AGENTS.md`.
#[test]
fn leaves_an_earlier_install_alone() {
	let agents = format!("# Project\n\n{FALLBACK}\n");
	let claude = format!("# Claude\n\n{FALLBACK}\n");
	assert_eq!(
		instruction_files(Some(&agents), Some(&claude)),
		Ok((None, None))
	);
	assert_eq!(
		instruction_files(Some(&agents), Some("# Claude\n")),
		Ok((None, None))
	);
	assert_eq!(
		instruction_files(Some("# Project\n"), Some(&claude)),
		Ok((None, None))
	);
}

#[test]
fn refuses_separate_agents_and_claude_md() {
	assert!(instruction_files(Some("# Project\n"), Some("# Claude\n")).is_err());
	assert!(instruction_files(Some("# Project\n"), Some("Read AGENTS.md.\n")).is_err());
}

/// An import does not move into `AGENTS.md`, where it would import the file itself.
#[test]
fn keeps_an_importing_claude_md_when_agents_md_is_missing() {
	let claude = "@AGENTS.md\n\nUse Opus for reviews.\n";
	assert_eq!(
		instruction_files(None, Some(claude)),
		Ok((Some(format!("{FALLBACK}\n")), None))
	);
}

#[test]
fn treats_an_empty_agents_md_as_missing() {
	let claude = "# Claude\n";
	for agents in ["", "\n \n"] {
		assert_eq!(
			instruction_files(Some(agents), Some(claude)),
			Ok((Some(with_fallback(claude)), Some(CLAUDE_IMPORT.to_owned()))),
			"{agents:?}"
		);
	}
}

#[test]
fn accepts_an_import_with_a_relative_path() {
	assert_eq!(
		instruction_files(Some("# Project\n"), Some("@./AGENTS.md\n")),
		Ok((Some(with_fallback("# Project\n")), None))
	);
}

/// An empty `CLAUDE.md` holds nothing to merge, so replacing it loses nothing.
#[test]
fn replaces_an_empty_claude_md_with_the_import() {
	for claude in ["", "\n \n"] {
		assert_eq!(
			instruction_files(Some("# Project\n"), Some(claude)),
			Ok((
				Some(with_fallback("# Project\n")),
				Some(CLAUDE_IMPORT.to_owned())
			)),
			"{claude:?}"
		);
	}
}

#[test]
fn prepending_keeps_the_existing_text_byte_for_byte() {
	let agents = "# Project\r\n\n  indented\ttext without a final newline";
	let (new, _) = instruction_files(Some(agents), Some(CLAUDE_IMPORT)).expect("no refusal");
	let new = new.expect("AGENTS.md changes");
	let rest = new
		.strip_prefix(FALLBACK)
		.and_then(|rest| rest.strip_prefix("\n\n"))
		.expect("the fallback comes first");
	assert_eq!(rest, agents);
}

#[test]
fn an_empty_agents_md_gets_only_the_fallback() {
	for agents in ["", "\n", " \n\n"] {
		assert_eq!(
			instruction_files(Some(agents), Some(CLAUDE_IMPORT)),
			Ok((Some(format!("{FALLBACK}\n")), None)),
			"{agents:?}"
		);
	}
}

#[test]
fn running_again_on_the_output_changes_nothing() {
	let cases: [(Option<&str>, Option<&str>); 9] = [
		(None, Some("@AGENTS.md\n\nUse Opus for reviews.\n")),
		(Some(""), Some("# Claude\n")),
		(Some("# Project\n"), Some("@./AGENTS.md\n")),
		(None, None),
		(None, Some("# Claude\n")),
		(Some("# Project\n"), None),
		(Some("# Project\n"), Some(CLAUDE_IMPORT)),
		(Some("# Project\n"), Some("")),
		(Some(""), Some(CLAUDE_IMPORT)),
	];
	for (agents, claude) in cases {
		let (new_agents, new_claude) = instruction_files(agents, claude).expect("no refusal");
		let agents = new_agents.as_deref().or(agents);
		let claude = new_claude.as_deref().or(claude);
		assert_eq!(
			instruction_files(agents, claude),
			Ok((None, None)),
			"{agents:?} {claude:?}"
		);
	}
}

/// Without a matcher the hooks run for every session source, `fork` included.
#[test]
fn templates_have_no_matcher() {
	for (path, template, event, _) in configs() {
		let entry = &parse(template)["hooks"][event][0];
		assert!(entry.get("matcher").is_none(), "{path}");
	}
}

#[test]
fn user_template_runs_the_user_loader() {
	let entry = &parse(USER_CONFIG.1)["hooks"]["SessionStart"][0];
	assert_eq!(
		entry["hooks"][0]["command"],
		"\"$HOME\"/.claude/agent-guidance/load.sh claude user"
	);
	assert_eq!(entry["hooks"][0]["timeout"], 15);
	assert!(!USER_CONFIG.1.contains(LOADER_PATH));
}

/// The user hook and a repository hook in one config are separate entries: merging one keeps
/// the other.
#[test]
fn user_and_repository_entries_are_independent() {
	let (_, repository, event) = HOOK_CONFIGS[0];
	let user = USER_CONFIG.1;
	let both = merge_hook(repository, user, event, USER_LOADER_PATH)
		.unwrap()
		.unwrap();
	assert_eq!(
		parse(&both)["hooks"][event],
		Value::Array(vec![
			template_entry(repository, event),
			template_entry(user, event)
		])
	);
	assert_eq!(merge_hook(&both, user, event, USER_LOADER_PATH), Ok(None));
	assert_eq!(merge_hook(&both, repository, event, LOADER_PATH), Ok(None));
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
	let (_, template, event, loader) = configs()[0];
	let existing = r#"{
		"permissions": {"allow": ["Bash(ls)"]},
		"hooks": {
			"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "a"}]}],
			"SessionStart": [{"hooks": [{"type": "command", "command": "b"}]}]
		},
		"model": "x"
	}"#;
	let merged = parse(
		&merge_hook(existing, template, event, loader)
			.unwrap()
			.unwrap(),
	);
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
	for (path, template, event, loader) in configs() {
		let existing = r#"{"hooks": {"Other": []}}"#;
		let once = merge_hook(existing, template, event, loader)
			.unwrap()
			.unwrap();
		assert_eq!(
			merge_hook(&once, template, event, loader),
			Ok(None),
			"{path}"
		);
	}
}

#[test]
fn adds_missing_cursor_version_and_keeps_an_existing_one() {
	let (_, template, event, loader) = configs()[3];
	let added = parse(
		&merge_hook(r#"{"hooks": {}}"#, template, event, loader)
			.unwrap()
			.unwrap(),
	);
	assert_eq!(added["version"], 1);
	let kept = r#"{"version": 2, "hooks": {"stop": [{"command": "x"}]}}"#;
	let kept = parse(&merge_hook(kept, template, event, loader).unwrap().unwrap());
	assert_eq!(kept["version"], 2);
	assert_eq!(kept["hooks"]["stop"][0]["command"], "x");
}

#[test]
fn rejects_configs_it_cannot_merge() {
	let (_, template, event, loader) = configs()[0];
	for existing in [
		"",
		"{",
		"{} trailing",
		"[]",
		r#"{"hooks": []}"#,
		r#"{"hooks": {"SessionStart": {}}}"#,
	] {
		assert!(
			merge_hook(existing, template, event, loader).is_err(),
			"{existing:?}"
		);
	}
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

/// The loader entry of `template` for `event`.
fn template_entry(template: &str, event: &str) -> Value {
	parse(template)["hooks"][event][0].clone()
}

/// An entry of the same shape as `entry` that runs `command` instead of the loader.
fn other_entry(entry: &Value, command: &str) -> Value {
	rewrite(entry, "command", &Value::from(command))
}

/// `entry` with a hook the user added next to the loader.
fn mixed_entry(entry: &Value) -> Value {
	let mut entry = entry.clone();
	match entry.get_mut("hooks").and_then(Value::as_array_mut) {
		Some(hooks) => hooks.push(serde_json::json!({"type": "command", "command": "echo hello"})),
		// A Cursor entry has one command, so here the loader path is in another key.
		None => {
			entry["note"] = entry["command"].clone();
			entry["command"] = Value::from("echo hello");
		}
	}
	entry
}

/// A config with `entries` for `event`, between two other keys.
fn config_with(event: &str, entries: &[Value]) -> String {
	serde_json::json!({"first": 1, "hooks": {"Other": [], event: entries}, "last": 2}).to_string()
}

#[test]
fn replaces_an_outdated_loader_entry_in_place() {
	for (path, template, event, loader) in configs() {
		let entry = template_entry(template, event);
		let before = other_entry(&entry, "before");
		let after = other_entry(&entry, "after");
		for outdated in [
			rewrite(&entry, "timeout", &Value::from(1)),
			other_entry(&entry, &format!("bash ./{loader} --old")),
		] {
			let existing = config_with(event, &[before.clone(), outdated, after.clone()]);
			let merged = parse(
				&merge_hook(&existing, template, event, loader)
					.unwrap_or_else(|e| panic!("{path}: {e}"))
					.unwrap_or_else(|| panic!("{path}: nothing changed")),
			);
			// Template keys the config lacks, such as Cursor's `version`, follow the existing ones.
			let keys: Vec<&String> = merged.as_object().unwrap().keys().collect();
			assert_eq!(keys[..3], ["first", "hooks", "last"], "{path}");
			let hook_keys: Vec<&String> = merged["hooks"].as_object().unwrap().keys().collect();
			assert_eq!(hook_keys, ["Other", event], "{path}");
			assert_eq!(
				merged["hooks"][event],
				Value::Array(vec![before.clone(), entry.clone(), after.clone()]),
				"{path}"
			);
			assert_eq!(
				merge_hook(&merged.to_string(), template, event, loader),
				Ok(None),
				"{path}"
			);
		}
	}
}

#[test]
fn collapses_duplicate_loader_entries() {
	for (path, template, event, loader) in configs() {
		let entry = template_entry(template, event);
		let outdated = rewrite(&entry, "timeout", &Value::from(1));
		let other = other_entry(&entry, "other");
		for entries in [
			[outdated.clone(), other.clone(), entry.clone()],
			[other.clone(), entry.clone(), entry.clone()],
		] {
			let position = entries.iter().position(|e| *e != other).unwrap();
			let existing = config_with(event, &entries);
			let merged = parse(
				&merge_hook(&existing, template, event, loader)
					.unwrap_or_else(|e| panic!("{path}: {e}"))
					.unwrap_or_else(|| panic!("{path}: nothing changed")),
			);
			let mut expected = vec![other.clone()];
			expected.insert(position, entry.clone());
			assert_eq!(merged["hooks"][event], Value::Array(expected), "{path}");
		}
	}
}

/// Replacing an entry that runs the loader next to other hooks would delete those hooks.
#[test]
fn rejects_a_loader_entry_with_other_hooks() {
	for (path, template, event, loader) in configs() {
		let entry = template_entry(template, event);
		let existing = config_with(event, &[mixed_entry(&entry)]);
		let error = merge_hook(&existing, template, event, loader).expect_err(path);
		assert_eq!(
			error,
			format!("an entry in `hooks.{event}` runs more than the loader"),
			"{path}"
		);
		// A second, loader-only entry does not make the mixed one safe to remove.
		let existing = config_with(event, &[entry.clone(), mixed_entry(&entry)]);
		assert!(
			merge_hook(&existing, template, event, loader).is_err(),
			"{path}"
		);
	}
}

/// The command of a template's loader entry.
fn template_command(entry: &Value) -> String {
	entry
		.pointer("/hooks/0/command")
		.or_else(|| entry.get("command"))
		.and_then(Value::as_str)
		.expect("a loader command")
		.to_owned()
}

/// Hooks without a `command`, such as an HTTP hook, also count as other hooks.
#[test]
fn rejects_a_loader_group_with_hooks_of_other_types() {
	for (path, template, event, loader) in configs() {
		let entry = template_entry(template, event);
		if entry.get("hooks").is_none() {
			continue;
		}
		for extra in [
			serde_json::json!({"type": "http", "url": "https://example.com/hook"}),
			serde_json::json!({"type": "prompt", "command": loader}),
			serde_json::json!({}),
		] {
			let mut mixed = entry.clone();
			mixed["hooks"].as_array_mut().unwrap().push(extra.clone());
			let existing = config_with(event, &[mixed]);
			assert!(
				merge_hook(&existing, template, event, loader).is_err(),
				"{path}: {extra}"
			);
		}
		let mut empty = entry.clone();
		empty["hooks"] = Value::Array(Vec::new());
		empty["note"] = Value::from(loader);
		let existing = config_with(event, &[empty]);
		assert!(
			merge_hook(&existing, template, event, loader).is_err(),
			"{path}"
		);
	}
}

/// A command that runs the loader and chains another command runs more than the loader.
#[test]
fn rejects_a_loader_command_that_runs_other_commands() {
	for (path, template, event, loader) in configs() {
		let entry = template_entry(template, event);
		let command = template_command(&entry);
		for chained in [
			format!("{command} && notify-me"),
			format!("{command}; notify-me"),
			format!("{command} || notify-me"),
			format!("{command} | tee log"),
			format!("{command} &"),
			format!("{command}\nnotify-me"),
			format!("{command} `notify-me`"),
			format!("{command} $(notify-me)"),
			format!("{command} <(notify-me)"),
			format!("{command} >(notify-me)"),
			format!("bash -c 'x; ./{loader}'"),
		] {
			let changed = rewrite(&entry, "command", &Value::from(chained.clone()));
			let existing = config_with(event, &[changed]);
			assert!(
				merge_hook(&existing, template, event, loader).is_err(),
				"{path}: {chained:?}"
			);
		}
	}
}

/// Every template entry counts as loader-only, so an outdated copy of it can be replaced.
#[test]
fn template_entries_run_only_the_loader() {
	for (path, template, event, loader) in configs() {
		assert!(
			loader_only(&template_entry(template, event), loader),
			"{path}"
		);
	}
}
