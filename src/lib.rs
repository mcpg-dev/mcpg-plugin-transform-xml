//! XML ⇄ JSON transform plugin.
//!
//! Stateless apart from the manifest — direction + options arrive per call in
//! `config`, so one instance serves both the global transform chain and the
//! pipeline `plugin_transform` bridge. Pure compute; no I/O.
//!
//! Convention (a compact xmltodict-style mapping):
//! - A text-only element becomes a JSON string.
//! - An element with attributes/children becomes an object: attributes keyed
//!   `@name`, mixed text under `#text`, child elements keyed by tag name with
//!   repeated tags collapsed to a JSON array.
//! - `xml_to_json` wraps the document in its root tag, so `json_to_xml` of the
//!   result round-trips.

mod xml;

use mcpg_plugin_protocol::{PluginContext, PluginManifest, TransformResult, firstparty_manifest};
use mcpg_plugin_sdk::ffi::SyncTransform;
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Arguments,
    Result,
    #[default]
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    XmlToJson,
    JsonToXml,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct XmlConfig {
    direction: Direction,
    /// `json_to_xml` root element name. When omitted, the input must be an
    /// object with exactly one top-level key, used as the root element.
    #[serde(default)]
    root_name: Option<String>,
    /// JSON Pointer (RFC 6901) to the sub-value to transform. When omitted the
    /// whole value is transformed.
    #[serde(default)]
    pointer: Option<String>,
    #[serde(default)]
    phase: Phase,
    #[serde(default = "default_max_output_bytes")]
    max_output_bytes: usize,
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

pub struct XmlTransform {
    manifest: PluginManifest,
}

impl XmlTransform {
    pub fn new(_config_json: &str) -> Self {
        Self {
            manifest: firstparty_manifest! {
                id: "dev.mcpg.transform.xml",
                name: "XML Transform",
                class: Transform,
            },
        }
    }

    fn run(&self, value: &Value, config: &Value, phase: Phase) -> TransformResult {
        let cfg: XmlConfig = match serde_json::from_value(config.clone()) {
            Ok(c) => c,
            Err(e) => {
                return TransformResult::Error {
                    message: format!("xml transform config: {e}"),
                };
            }
        };
        if cfg.phase != Phase::Both && cfg.phase != phase {
            return TransformResult::Unchanged;
        }

        let ptr = cfg.pointer.as_deref().unwrap_or("");
        let target = match value.pointer(ptr) {
            Some(t) => t,
            None => {
                return TransformResult::Error {
                    message: format!("pointer {ptr:?} not found in value"),
                };
            }
        };

        let produced = match cfg.direction {
            Direction::XmlToJson => match target.as_str() {
                Some(s) => xml::xml_to_json(s),
                None => Err("xml_to_json: input value must be a string".to_owned()),
            },
            Direction::JsonToXml => {
                xml::json_to_xml(target, cfg.root_name.as_deref()).map(Value::String)
            }
        };
        let produced = match produced {
            Ok(v) => v,
            Err(message) => return TransformResult::Error { message },
        };

        match serde_json::to_string(&produced) {
            Ok(s) if s.len() > cfg.max_output_bytes => {
                return TransformResult::Error {
                    message: format!(
                        "xml output {} bytes exceeds max_output_bytes ({})",
                        s.len(),
                        cfg.max_output_bytes
                    ),
                };
            }
            Ok(_) => {}
            Err(e) => {
                return TransformResult::Error {
                    message: format!("output encode: {e}"),
                };
            }
        }

        if ptr.is_empty() {
            TransformResult::Modified { value: produced }
        } else {
            let mut out = value.clone();
            match out.pointer_mut(ptr) {
                Some(slot) => {
                    *slot = produced;
                    TransformResult::Modified { value: out }
                }
                None => TransformResult::Error {
                    message: format!("pointer {ptr:?} not assignable"),
                },
            }
        }
    }
}

impl SyncTransform for XmlTransform {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn transform_arguments(
        &self,
        _ctx: &PluginContext,
        arguments: &Value,
        config: &Value,
    ) -> TransformResult {
        self.run(arguments, config, Phase::Arguments)
    }

    fn transform_result(
        &self,
        _ctx: &PluginContext,
        result: &Value,
        config: &Value,
    ) -> TransformResult {
        self.run(result, config, Phase::Result)
    }
}

#[cfg(any(feature = "cdylib-export", feature = "static-firstparty"))]
mcpg_plugin_sdk::declare_plugin! {
    plugin_id: "dev.mcpg.transform.xml",
    plugin_version: env!("CARGO_PKG_VERSION"),
    descriptor_yaml: include_str!("../plugin.yaml"),
    capabilities: &[],
    entities: [
        transform as xform {
            inner_name: "",
            plugin_type: XmlTransform,
            factory: |cfg: &str, _host: ::mcpg_plugin_sdk::HostHandle| XmlTransform::new(cfg),
        },
    ],
}

#[cfg(test)]
mod tests;
