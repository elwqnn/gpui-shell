//! The config seen as a flat list of values, so a UI can show and change it.
//!
//! Reading goes through `toml::Value`: [`Config`] already serializes to one,
//! which spares the shell a second description of its own settings, and
//! [`flatten`] turns that tree into `bar.position`-style rows.
//!
//! Writing goes back into `config.toml`, which stays the one source of truth.
//! The file is edited in place with `toml_edit`, so comments, key order and
//! formatting survive a change made from the UI - only the line that changed
//! changes.

use std::fs;

use anyhow::Context as _;
use toml::Value;
use toml_edit::{DocumentMut, Item, Table};

use super::{Config, persistence};

/// One editable value, keyed by the dotted path both files agree on.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub path: String,
    pub value: Value,
}

impl Setting {
    /// The top-level table the value lives in (`bar`, `launcher`, ...), or
    /// `general` for one that sits at the root of the file.
    pub fn section(&self) -> &str {
        match self.path.split_once('.') {
            Some((section, _)) => section,
            None => "general",
        }
    }

    /// The path without its section, which is what a section's rows show.
    pub fn label(&self) -> &str {
        self.path
            .split_once('.')
            .map_or(self.path.as_str(), |(_, rest)| rest)
    }

    /// The value as it would be written in a TOML file.
    pub fn text(&self) -> String {
        self.value.to_string()
    }
}

/// Every leaf of the config, in declaration order. Tables recurse; arrays and
/// inline tables stay whole - they are edited as the TOML text they are.
pub fn flatten(config: &Config) -> Vec<Setting> {
    fn walk(prefix: &str, value: &Value, out: &mut Vec<Setting>) {
        match value {
            Value::Table(table) => {
                for (key, value) in table {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&path, value, out);
                }
            }
            leaf => out.push(Setting {
                path: prefix.to_string(),
                value: leaf.clone(),
            }),
        }
    }

    let mut out = Vec::new();
    match Value::try_from(config) {
        Ok(value) => walk("", &value, &mut out),
        Err(err) => tracing::warn!("Failed to read the config as TOML: {err}"),
    }
    out
}

/// Parse what the user typed into the value a row holds. Bare words are the
/// common case - `top`, `Adwaita` - and TOML only accepts those quoted, so a
/// failed parse is read as the string it looks like.
pub fn parse_value(text: &str) -> Value {
    let text = text.trim();
    toml::from_str::<Value>(&format!("value = {text}"))
        .ok()
        .and_then(|parsed| parsed.get("value").cloned())
        .unwrap_or_else(|| Value::String(text.to_string()))
}

/// Change one value in `config.toml` and give back the config it now holds.
/// `None` removes the key, which hands the value back to its default.
pub fn write(path: &str, value: Option<Value>) -> anyhow::Result<Config> {
    let file = persistence::config_path()?;
    let raw = match fs::read_to_string(&file) {
        Ok(raw) => raw,
        // Nothing written yet: the defaults are what the UI is editing.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err).context(format!("Failed to read {}", file.display())),
    };

    let (edited, config) = edit_document(&raw, path, value)?;
    fs::write(&file, &edited).with_context(|| format!("Failed to write {}", file.display()))?;
    Ok(config)
}

/// The file with `path` set to `value`, and the config it parses into. A value
/// the config cannot hold fails here, before anything reaches the disk - the
/// hand-written file must not be left holding something that will not load.
fn edit_document(raw: &str, path: &str, value: Option<Value>) -> anyhow::Result<(String, Config)> {
    let mut document: DocumentMut = raw
        .parse()
        .with_context(|| "config.toml has to parse before the UI can change it")?;

    let entry = match value {
        Some(value) => Some(
            value
                .to_string()
                .parse::<toml_edit::Value>()
                .with_context(|| format!("{value} is not a value TOML can write"))?,
        ),
        None => None,
    };

    set_path(document.as_table_mut(), path, entry);

    let edited = document.to_string();
    let config = toml::from_str(&edited).context("that value does not fit the setting")?;
    Ok((edited, config))
}

/// Write (or remove) a dotted key, creating the tables it needs on the way.
/// Tables created here are implicit: a change to `bar.modules.clock.format`
/// adds one `[bar.modules.clock]` header, not three.
fn set_path(table: &mut Table, path: &str, value: Option<toml_edit::Value>) {
    let Some((head, rest)) = path.split_once('.') else {
        match value {
            Some(value) => {
                // Assigning through the entry keeps whatever decor - the
                // comment at the end of the line - the key already had.
                let item = table
                    .entry(path)
                    .or_insert_with(|| Item::Value(value.clone()));
                if let Some(existing) = item.as_value_mut() {
                    let decor = existing.decor().clone();
                    *existing = value;
                    *existing.decor_mut() = decor;
                } else {
                    *item = Item::Value(value);
                }
            }
            None => {
                table.remove(path);
            }
        }
        return;
    };

    // A removal has no table to create: an absent key is already the default.
    if value.is_none() && !table.contains_key(head) {
        return;
    }

    let child = table.entry(head).or_insert_with(|| {
        let mut created = Table::new();
        created.set_implicit(true);
        Item::Table(created)
    });

    if let Some(child) = child.as_table_mut() {
        set_path(child, rest, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = concat!(
        "# my shell\n",
        "watch_config = true\n",
        "\n",
        "[bar]\n",
        "position = \"left\" # on the side\n",
        "size = 32.0\n",
    );

    #[test]
    fn every_scalar_becomes_one_dotted_row() {
        let settings = flatten(&Config::default());

        let position = settings
            .iter()
            .find(|setting| setting.path == "bar.position")
            .expect("the bar position is editable");
        assert_eq!(position.section(), "bar");
        assert_eq!(position.label(), "position");
        assert_eq!(position.text(), "\"left\"");

        let watch = settings
            .iter()
            .find(|setting| setting.path == "watch_config")
            .expect("a root value is editable too");
        assert_eq!(watch.section(), "general");
        assert_eq!(watch.label(), "watch_config");

        assert!(settings.iter().any(|s| s.path == "launcher.width"));
        assert!(settings.iter().all(|s| !s.path.is_empty()));
    }

    /// The file is hand-written: a change from the UI has to leave the parts
    /// it did not touch - comments included - exactly as they were.
    #[test]
    fn a_change_rewrites_one_line_and_nothing_else() {
        let (edited, config) =
            edit_document(FILE, "bar.position", Some(Value::String("top".into())))
                .expect("a position the enum has");

        assert_eq!(config.bar.position, crate::config::BarPosition::Top);
        assert!(edited.contains("# my shell"), "{edited}");
        assert!(
            edited.contains("position = \"top\" # on the side"),
            "{edited}"
        );
        assert!(edited.contains("size = 32.0"), "{edited}");
        assert!(edited.contains("watch_config = true"), "{edited}");
    }

    /// A setting the file never mentioned gets its table written for it, and
    /// only the one table.
    #[test]
    fn a_new_setting_brings_its_table_with_it() {
        let (edited, config) = edit_document(FILE, "launcher.width", Some(Value::Float(720.0)))
            .expect("a width the launcher accepts");

        assert_eq!(config.launcher.width, 720.0);
        assert_eq!(edited.matches('[').count(), 2, "{edited}");
        assert!(edited.contains("[launcher]"), "{edited}");
    }

    /// Removing the key is how a value goes back to its default.
    #[test]
    fn removing_a_key_restores_the_default() {
        let (edited, config) =
            edit_document(FILE, "bar.position", None).expect("removal always parses");

        assert_eq!(config.bar.position, Config::default().bar.position);
        assert!(!edited.contains("position"), "{edited}");
        assert!(edited.contains("size = 32.0"), "{edited}");
    }

    /// The panel validates by parsing: a value the config cannot hold has to
    /// fail here, or it would be written to a file that then stops loading.
    #[test]
    fn a_value_the_config_cannot_hold_never_reaches_the_file() {
        let refused = edit_document(FILE, "bar.position", Some(Value::String("sideways".into())));

        assert!(refused.is_err(), "an unknown position must not be written");
    }

    #[test]
    fn typed_text_parses_as_toml_and_falls_back_to_a_string() {
        assert_eq!(parse_value("36"), Value::Integer(36));
        assert_eq!(parse_value("true"), Value::Boolean(true));
        assert_eq!(parse_value("\"top\""), Value::String("top".into()));
        // Bare words are what a user types for an enum or a name.
        assert_eq!(parse_value("top"), Value::String("top".into()));
        assert_eq!(parse_value("%H:%M"), Value::String("%H:%M".into()));
    }
}
