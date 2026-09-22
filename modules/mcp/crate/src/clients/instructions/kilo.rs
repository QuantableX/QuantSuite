//! Kilo's global instructions are references in JSONC, shared by CLI and VS Code.
use super::{env_path, read_optional};
use jsonc_parser::{cst::CstRootNode, json, ParseOptions};
use std::fs;
use std::path::{Path, PathBuf};

fn config_in(directory: &Path) -> PathBuf {
    // Match Kilo's globalConfigFile() preference, including migrated installs.
    [
        "kilo.jsonc",
        "kilo.json",
        "opencode.jsonc",
        "opencode.json",
        "config.json",
    ]
    .iter()
    .map(|name| directory.join(name))
    .find(|path| path.exists())
    .unwrap_or_else(|| directory.join("kilo.jsonc"))
}

pub(super) fn config_path() -> Result<PathBuf, String> {
    let directory = env_path("XDG_CONFIG_HOME")
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .ok_or("Could not resolve Kilo's config directory")?
        .join("kilo");
    Ok(config_in(&directory))
}

fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.replace('\\', "/")
            .eq_ignore_ascii_case(&b.replace('\\', "/"))
    } else {
        a == b
    }
}

fn add_instruction(existing: &str, source: &str) -> Result<String, String> {
    // Match JSONC, not permissive JSON5. Never echo config contents in errors.
    let options = ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    let root = CstRootNode::parse(existing, &options)
        .map_err(|_| "Invalid Kilo JSONC; file left untouched")?;
    let object = root
        .object_value_or_create()
        .ok_or("Kilo config must be an object; file left untouched")?;
    let properties: Vec<_> = object
        .properties()
        .into_iter()
        .filter(|prop| prop.decoded_name().as_deref() == Some("instructions"))
        .collect();
    match properties.as_slice() {
        [] => {
            object.append("instructions", json!([source]));
        }
        [property] => {
            let array = property
                .value()
                .and_then(|value| value.as_array())
                .ok_or("Kilo instructions must be an array; file left untouched")?;
            let mut found = false;
            for value in array.elements() {
                let entry = value
                    .as_string_lit()
                    .and_then(|value| value.decoded_value().ok())
                    .ok_or("Kilo instructions must contain only strings; file left untouched")?;
                found |= same_path(&entry, source);
            }
            if found {
                return Ok(existing.to_owned());
            }
            array.append(json!(source));
        }
        _ => return Err("Duplicate Kilo instructions keys; file left untouched".into()),
    }
    Ok(root.to_string())
}

pub(super) fn install(path: &Path, source: &Path) -> Result<String, String> {
    if !path.is_absolute() || !source.is_absolute() {
        return Err("Kilo config and AgentOS source paths must be absolute".into());
    }
    let destination = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if destination == fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf()) {
        return Err("Kilo config points at the AgentOS source; file left untouched".into());
    }
    let old = read_optional(&destination)?;
    let source_text = source.to_str().ok_or("AgentOS source path is not UTF-8")?;
    // Kilo accepts glob paths; forward slashes avoid Windows escape semantics.
    let source_text = if cfg!(windows) {
        source_text.replace('\\', "/")
    } else {
        source_text.to_owned()
    };
    let new =
        add_instruction(&old, &source_text).map_err(|e| format!("{}: {e}", path.display()))?;
    if old == new {
        return Ok(format!(
            "Up to date: {} (shared by Kilo Code and Kilo CLI)",
            path.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    qs_core::paths::write_atomic_with_backup(&destination, new.as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(format!(
        "Linked General AgentOS in {} (shared by Kilo Code and Kilo CLI)",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    #[test]
    fn preserves_jsonc_comments_settings_and_existing_instructions() {
        let old = "// Personal config\r\n{\r\n  \"model\": \"keep/model\", // keep me\r\n  \"instructions\": [\r\n    \"personal.md\", // personal rule\r\n  ],\r\n}\r\n";
        let source = "C:\\Users\\A User\\.quantmcp\\AGENT.md";
        let new = add_instruction(old, source).unwrap();
        for preserved in [
            "// Personal config\r\n",
            "\"model\": \"keep/model\", // keep me",
            "\"personal.md\", // personal rule",
        ] {
            assert!(new.contains(preserved), "Missing {preserved}");
        }
        assert_eq!(add_instruction(&new, source).unwrap(), new);
        if cfg!(windows) {
            assert_eq!(
                add_instruction(&new, &source.replace('\\', "/").to_lowercase()).unwrap(),
                new
            );
        }
    }

    #[test]
    fn creates_missing_array_without_removing_settings_or_comments() {
        for old in ["", "// comment\n", "{}", "{\"model\":\"kept\"}"] {
            let new = add_instruction(old, "/home/user/.quantmcp/AGENT.md").unwrap();
            assert!(new.contains("instructions"));
            assert!(new.contains("/home/user/.quantmcp/AGENT.md"));
            if old.contains("kept") {
                assert!(new.contains("kept"));
            }
            if old.contains("comment") {
                assert!(new.contains("// comment"));
            }
        }
    }

    #[test]
    fn rejects_ambiguous_or_invalid_config_even_if_source_already_present() {
        for old in [
            "[]",
            "null",
            "{broken}",
            "{\"instructions\":null}",
            "{\"instructions\":{}}",
            "{\"instructions\":[\"source\",42]}",
            "{\"instructions\":[],\"instructions\":[]}",
            "{\"instructions\":[],\"instr\\u0075ctions\":[]}",
            "{\"secret\":\"hidden\" \"instructions\":[]}",
        ] {
            let error = add_instruction(old, "source").unwrap_err();
            assert!(!error.contains("hidden"));
        }
    }

    #[test]
    fn install_backs_up_is_idempotent_and_leaves_invalid_config_untouched() {
        let temp = Scratch::new();
        let path = temp.0.join("kilo.jsonc");
        let source = temp.0.join("AGENT.md");
        fs::write(&source, "AgentOS policy").unwrap();
        let old = "// retained\n{\"model\":\"kept\"}";
        fs::write(&path, old).unwrap();
        install(&path, &source).unwrap();
        let imported = fs::read_to_string(&path).unwrap();
        assert!(imported.contains(&source.to_str().unwrap().replace('\\', "/")));
        assert_eq!(
            fs::read_to_string(qs_core::paths::backup_path(&path)).unwrap(),
            old
        );
        assert!(install(&path, &source).unwrap().starts_with("Up to date"));
        assert_eq!(
            fs::read_to_string(qs_core::paths::backup_path(&path)).unwrap(),
            old
        );
        let invalid = "{\"instructions\":false}";
        fs::write(&path, invalid).unwrap();
        assert!(install(&path, &source).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        assert_eq!(fs::read_to_string(&source).unwrap(), "AgentOS policy");
        assert!(install(&source, &source).is_err());
    }

    #[test]
    fn uses_existing_global_config_with_kilos_precedence() {
        let temp = Scratch::new();
        assert_eq!(config_in(&temp.0), temp.0.join("kilo.jsonc"));
        for name in [
            "config.json",
            "opencode.json",
            "opencode.jsonc",
            "kilo.json",
            "kilo.jsonc",
        ] {
            fs::write(temp.0.join(name), "{}").unwrap();
            assert_eq!(config_in(&temp.0), temp.0.join(name));
        }
    }
}
