use mcpg_plugin_protocol::{PluginContext, PluginIdentity, TransformResult};
use mcpg_plugin_sdk::ffi::SyncTransform;
use serde_json::{Value, json};

use super::XmlTransform;
use super::xml::{json_to_xml, xml_to_json};

fn ctx() -> PluginContext {
    PluginContext {
        request_id: "t".into(),
        session_id: None,
        tool_name: "x".into(),
        surface: "tool".into(),
        identity: PluginIdentity {
            kind: "anonymous".into(),
            trust_level: "unauthenticated".into(),
            subject_id: None,
            auth_provider: None,
            issuer: None,
            roles: Vec::new(),
            groups: Vec::new(),
            scopes: Vec::new(),
            attributes: Default::default(),
        },
        transport: "http".into(),
    }
}

// --- xml_to_json ------------------------------------------------------------

#[test]
fn text_only_element() {
    assert_eq!(xml_to_json("<a>hi</a>").unwrap(), json!({ "a": "hi" }));
}

#[test]
fn attributes_and_children() {
    let v = xml_to_json(r#"<note id="1"><to>Tove</to><from>Jani</from></note>"#).unwrap();
    assert_eq!(
        v,
        json!({ "note": { "@id": "1", "to": "Tove", "from": "Jani" } })
    );
}

#[test]
fn repeated_children_become_array() {
    let v = xml_to_json("<list><item>a</item><item>b</item></list>").unwrap();
    assert_eq!(v, json!({ "list": { "item": ["a", "b"] } }));
}

#[test]
fn mixed_attr_and_text() {
    let v = xml_to_json(r#"<p class="x">hello</p>"#).unwrap();
    assert_eq!(v, json!({ "p": { "@class": "x", "#text": "hello" } }));
}

#[test]
fn entity_in_text_is_resolved() {
    assert_eq!(
        xml_to_json("<a>a &amp; b &lt; c</a>").unwrap(),
        json!({ "a": "a & b < c" })
    );
}

#[test]
fn self_closing_element_is_empty_string() {
    assert_eq!(xml_to_json("<a/>").unwrap(), json!({ "a": "" }));
}

#[test]
fn cdata_is_captured_raw() {
    assert_eq!(
        xml_to_json("<a><![CDATA[<raw>&]]></a>").unwrap(),
        json!({ "a": "<raw>&" })
    );
}

#[test]
fn malformed_xml_is_error() {
    assert!(xml_to_json("<a><b></a>").is_err());
}

// --- json_to_xml ------------------------------------------------------------

#[test]
fn json_to_xml_simple() {
    assert_eq!(
        json_to_xml(&json!({ "a": "hi" }), None).unwrap(),
        "<a>hi</a>"
    );
}

#[test]
fn json_to_xml_attrs_and_children() {
    let out = json_to_xml(&json!({ "note": { "@id": "1", "to": "Tove" } }), None).unwrap();
    assert!(out.starts_with("<note id=\"1\">"), "{out}");
    assert!(out.contains("<to>Tove</to>"), "{out}");
    assert!(out.ends_with("</note>"), "{out}");
}

#[test]
fn json_to_xml_array_repeats() {
    assert_eq!(
        json_to_xml(&json!({ "list": { "item": ["a", "b"] } }), None).unwrap(),
        "<list><item>a</item><item>b</item></list>"
    );
}

#[test]
fn json_to_xml_escapes_text() {
    assert_eq!(
        json_to_xml(&json!({ "a": "x < y & z" }), None).unwrap(),
        "<a>x &lt; y &amp; z</a>"
    );
}

#[test]
fn json_to_xml_empty_object_self_closes() {
    assert_eq!(json_to_xml(&json!({ "a": {} }), None).unwrap(), "<a/>");
}

#[test]
fn json_to_xml_root_name_wraps() {
    assert_eq!(
        json_to_xml(&json!({ "x": "1" }), Some("doc")).unwrap(),
        "<doc><x>1</x></doc>"
    );
}

#[test]
fn json_to_xml_requires_single_root_without_root_name() {
    assert!(json_to_xml(&json!({ "a": 1, "b": 2 }), None).is_err());
}

#[test]
fn roundtrip_is_semantically_stable() {
    let xml = r#"<order id="42"><lines><line sku="A">2</line><line sku="B">5</line></lines><note>ship &amp; bill</note></order>"#;
    let j1 = xml_to_json(xml).unwrap();
    let xml2 = json_to_xml(&j1, None).unwrap();
    let j2 = xml_to_json(&xml2).unwrap();
    assert_eq!(j1, j2);
}

// --- via the SyncTransform surface -----------------------------------------

fn run_args(value: Value, cfg: Value) -> TransformResult {
    XmlTransform::new("{}").transform_arguments(&ctx(), &value, &cfg)
}

#[test]
fn transform_arguments_xml_to_json() {
    let r = run_args(json!("<a>hi</a>"), json!({ "direction": "xml_to_json" }));
    match r {
        TransformResult::Modified { value } => assert_eq!(value, json!({ "a": "hi" })),
        other => panic!("expected Modified, got {other:?}"),
    }
}

#[test]
fn transform_result_json_to_xml() {
    let r = XmlTransform::new("{}").transform_result(
        &ctx(),
        &json!({ "a": "hi" }),
        &json!({ "direction": "json_to_xml" }),
    );
    match r {
        TransformResult::Modified { value } => assert_eq!(value, json!("<a>hi</a>")),
        other => panic!("expected Modified, got {other:?}"),
    }
}

#[test]
fn pointer_targets_subfield() {
    let r = run_args(
        json!({ "payload": "<a>hi</a>", "keep": 1 }),
        json!({ "direction": "xml_to_json", "pointer": "/payload" }),
    );
    match r {
        TransformResult::Modified { value } => {
            assert_eq!(value, json!({ "payload": { "a": "hi" }, "keep": 1 }));
        }
        other => panic!("expected Modified, got {other:?}"),
    }
}

#[test]
fn bad_config_is_error() {
    // Missing required `direction`.
    assert!(matches!(
        run_args(json!("<a/>"), json!({})),
        TransformResult::Error { .. }
    ));
}

#[test]
fn phase_gating_skips_other_phase() {
    // direction set, phase=result → transform_arguments is a no-op.
    let r = run_args(
        json!("<a>hi</a>"),
        json!({ "direction": "xml_to_json", "phase": "result" }),
    );
    assert!(matches!(r, TransformResult::Unchanged));
}

#[test]
fn non_string_input_to_xml_to_json_errors() {
    assert!(matches!(
        run_args(
            json!({ "not": "a string" }),
            json!({ "direction": "xml_to_json" })
        ),
        TransformResult::Error { .. }
    ));
}
