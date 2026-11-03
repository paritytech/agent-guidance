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
