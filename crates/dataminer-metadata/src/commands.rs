//! The command tree, from `command_modules/mojang-commands.json`.
//!
//! This describes every slash command, its overloads and parameter types, plus
//! the command enums they reference. Ferrock needs it to build `AvailableCommands`
//! and to parse what a server sends back.

use dataminer_core::spec::SpecDocument;
use serde::{Deserialize, Serialize};

use crate::MetadataError;

/// The spec `kind` for command documents.
pub const KIND: &str = "commands";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

// ---- Raw shapes ------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawCommands {
    #[serde(default)]
    command_enums: Vec<RawEnum>,
    commands: Vec<RawCommand>,
}

#[derive(Debug, Deserialize)]
struct RawEnum {
    name: String,
    values: Vec<RawEnumValue>,
}

#[derive(Debug, Deserialize)]
struct RawEnumValue {
    value: String,
}

#[derive(Debug, Deserialize)]
struct RawCommand {
    name: String,
    #[serde(default)]
    description: String,
    // Mojang writes aliases as `[{ "name": "whitelist" }, ...]`, not bare strings.
    #[serde(default)]
    aliases: Vec<RawAlias>,
    #[serde(default)]
    permission_level: i64,
    #[serde(default)]
    requires_cheats: bool,
    #[serde(default)]
    overloads: Vec<RawOverload>,
}

#[derive(Debug, Deserialize)]
struct RawAlias {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawOverload {
    #[serde(default)]
    name: String,
    #[serde(default)]
    params: Vec<RawParam>,
}

#[derive(Debug, Deserialize)]
struct RawParam {
    name: String,
    #[serde(default)]
    is_optional: bool,
    #[serde(rename = "type")]
    ty: RawParamType,
}

#[derive(Debug, Deserialize)]
struct RawParamType {
    name: String,
}

// ---- Normalized payload ----------------------------------------------------

/// A command enum (a named set of literal values a parameter can take).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnum {
    /// The enum's name, e.g. `AimAssistActionSet`.
    pub name: String,
    /// Its values, in Mojang's order.
    pub values: Vec<String>,
}

/// One parameter of a command overload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    /// The parameter name shown to the player, e.g. `x angle`.
    pub name: String,
    /// Whether the parameter is optional.
    pub optional: bool,
    /// The parameter type's name (a primitive like `VAL`/`SELECTION`, or an enum
    /// name that appears in [`CommandsPayload::enums`]).
    pub type_name: String,
}

/// One overload (one valid argument signature) of a command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overload {
    /// Mojang's label for the overload.
    pub name: String,
    /// Its parameters, in order.
    pub params: Vec<Param>,
}

/// A slash command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// The command name, without the leading slash, e.g. `aimassist`.
    pub name: String,
    /// The one-line description.
    pub description: String,
    /// Alias names.
    pub aliases: Vec<String>,
    /// The required permission level.
    pub permission_level: i64,
    /// Whether the command requires cheats to be enabled.
    pub requires_cheats: bool,
    /// The overloads.
    pub overloads: Vec<Overload>,
}

/// The normalized command spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandsPayload {
    /// The command enums.
    pub enums: Vec<CommandEnum>,
    /// The commands.
    pub commands: Vec<Command>,
}

/// Parse `mojang-commands.json`.
pub fn parse(
    bytes: &[u8],
    game_version: &str,
    repo_ref: &str,
) -> Result<SpecDocument<CommandsPayload>, MetadataError> {
    const FILE: &str = "command_modules/mojang-commands.json";
    let raw: RawCommands =
        serde_json::from_slice(bytes).map_err(|source| MetadataError::Parse {
            file: FILE.to_string(),
            source,
        })?;
    if raw.commands.is_empty() {
        return Err(MetadataError::Sanity {
            file: FILE.to_string(),
            message: "no commands found; the file format may have changed".to_string(),
        });
    }

    let mut enums: Vec<CommandEnum> = raw
        .command_enums
        .into_iter()
        .map(|e| CommandEnum {
            name: e.name,
            values: e.values.into_iter().map(|v| v.value).collect(),
        })
        .collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));

    let mut commands: Vec<Command> = raw
        .commands
        .into_iter()
        .map(|c| Command {
            name: c.name,
            description: c.description,
            aliases: c.aliases.into_iter().map(|a| a.name).collect(),
            permission_level: c.permission_level,
            requires_cheats: c.requires_cheats,
            overloads: c
                .overloads
                .into_iter()
                .map(|o| Overload {
                    name: o.name,
                    params: o
                        .params
                        .into_iter()
                        .map(|p| Param {
                            name: p.name,
                            optional: p.is_optional,
                            type_name: p.ty.name,
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));

    let provenance = crate::provenance_for(game_version, repo_ref, FILE, bytes);
    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        CommandsPayload { enums, commands },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "command_enums": [ { "name": "AimAssistActionSet", "values": [ {"value":"set"} ] } ],
      "commands": [ {
        "aliases": [], "description": "Enable Aim Assist", "name": "aimassist",
        "permission_level": 1, "requires_cheats": true,
        "overloads": [ { "name": "1", "params": [
          {"is_optional": false, "name": "players", "type": {"name": "SELECTION"}},
          {"is_optional": true, "name": "x angle", "type": {"name": "VAL"}}
        ] } ]
      } ]
    }"#;

    #[test]
    fn parses_sample() {
        let doc = parse(SAMPLE.as_bytes(), "1.26.50.4", "v1.26.50.4").unwrap();
        assert_eq!(doc.payload.enums.len(), 1);
        assert_eq!(doc.payload.enums[0].values, vec!["set"]);
        let cmd = &doc.payload.commands[0];
        assert_eq!(cmd.name, "aimassist");
        assert!(cmd.requires_cheats);
        assert_eq!(cmd.overloads[0].params.len(), 2);
        assert_eq!(cmd.overloads[0].params[1].type_name, "VAL");
        assert!(cmd.overloads[0].params[1].optional);
    }
}
