# XML Transform — `dev.mcpg.transform.xml`

> class `transform` · `native` · package `mcpg-plugin-transform-xml` · artifact `libmcpg_plugin_transform_xml.so` · Apache-2.0

Converts between XML and JSON in either direction, so an MCP tool can speak JSON
to its callers while its backend speaks XML — SOAP envelopes, legacy enterprise
APIs, feed documents, vendor exports. `xml_to_json` parses an XML document into
a JSON tree; `json_to_xml` serialises that same shape back to an XML string. An
optional JSON Pointer narrows the conversion to a single sub-field so the rest
of the payload passes through untouched. Reach for it when a backend only
accepts or only returns XML and you do not want that leaking into your tool
schemas.

## What it does
- Parses an XML document into a JSON tree using a compact, round-trippable
  mapping, wrapped in the document's own root tag.
- Serialises that same JSON shape back to an XML string, escaping text and
  attribute values.
- Targets one sub-value with an RFC 6901 JSON Pointer (`pointer`), leaving the
  remainder of the payload untouched.
- Runs on tool arguments, on tool results, or on both, selected by `phase`.
- Rejects any conversion whose produced value serialises larger than
  `max_output_bytes`.
- Declares no `required_capabilities` — pure compute, with no network,
  filesystem, or host-service access.

## Configuration
Loaded from the flat top-level `plugins:` list. Every registered `transform`
plugin joins the gateway's global transform chain, applied in declaration order
to tool arguments before dispatch and to tool results after dispatch; the
entry's `config:` block is what that chain passes on every call.

```yaml
plugins:
  - id: dev.mcpg.transform.xml
    class: transform
    source:
      oci: ghcr.io/mcpg-dev/source-code/plugins/transform-xml:protocol-1
    config:
      direction: xml_to_json
      pointer: /body             # an XML string field carried on the arguments
      phase: arguments
      max_output_bytes: 1048576
```

| Field | Type | Default | Description |
|---|---|---|---|
| `direction` | `xml_to_json` \| `json_to_xml` | *(required)* | Conversion direction. |
| `root_name` | string | *(unset)* | Root element name for `json_to_xml`. When unset the input must be an object with exactly one top-level key, which becomes the root element. |
| `pointer` | string | *(whole value)* | RFC 6901 JSON Pointer to the sub-value to convert. An empty or absent pointer converts the whole value. |
| `phase` | `arguments` \| `result` \| `both` | `both` | Which dispatch phase the conversion fires on. |
| `max_output_bytes` | integer | `1048576` | Upper bound on the serialised size of the produced value. |

Unknown fields are rejected.

`xml_to_json` requires the targeted value to be a JSON string holding the XML
document; `json_to_xml` produces a JSON string. A config error, an unparseable
document, a pointer that does not resolve, or an oversized result produces a
transform error. In the global chain the gateway logs that error and passes the
value through unchanged; in a `plugin_transform` pipeline step the same error
fails the step.

## Mapping convention
The mapping is compact and round-trippable: `xml_to_json` output feeds straight
back into `json_to_xml`.

- A text-only element becomes a JSON string.
- An element carrying attributes or child elements becomes an object:
  attributes keyed `@name`, mixed text under `#text`, child elements keyed by
  tag name.
- Repeated child tags collapse into a JSON array, and an array serialises back
  to one element per item.
- An empty object serialises to a self-closing element.
- Comments, processing instructions, the XML declaration, and DOCTYPE
  declarations are skipped on read.

```yaml
# <note id="1"><to>Tove</to><to>Jani</to></note>
# ⇄ { "note": { "@id": "1", "to": ["Tove", "Jani"] } }
```

## Security
- Only the five predefined entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`,
  `&apos;`) and numeric character references are resolved. Any other `&name;`
  reference fails the conversion, and DTD declarations are skipped rather than
  processed, so no externally-defined or DTD-defined entity is ever expanded.
- `max_output_bytes` is checked against the serialised size of the produced
  value once the conversion finishes, so an oversized result fails the transform
  instead of travelling on. It bounds what leaves the plugin, not what the
  parser allocates while reading the document.
- The plugin declares no host capabilities and performs no I/O, so a malicious
  document has no network or filesystem reach.

## MCP surfaces & composition

### As a pipeline step
A `plugin_transform` step invokes this plugin by the alias its `plugins:` entry
carries in `id`. The step hands the plugin the whole pipeline context —
`arguments`, `tool_name`, `steps`, `context` — so `pointer` addresses into that
object, and the plugin's output becomes the step result. The step calls the
plugin's result-phase hook, so leave `phase` at its default or set it to
`result`.

```yaml
mcp:
  capabilities:
    tools:
      - name: order.lookup
        description: Fetch an order from a SOAP service and return JSON.
        backend:
          kind: pipeline
          steps:
            - kind: http
              id: fetch
              url: https://orders.example.com/soap/order
              method: post
            - kind: plugin_transform
              id: decode
              plugin: dev.mcpg.transform.xml
              config:
                direction: xml_to_json
                pointer: /steps/fetch/output/body
```

A pointer addresses a step's output through `steps/<step id>/output/…`, so the
step it names must run earlier in the list. With a pointer set the plugin
returns the context object with that one slot replaced — the step result is the
whole context, carrying the decoded value at the pointer's path.

## Build
Default feature set is OFF (avoids `mcpg_plugin_register` linker
collisions in the workspace build); opt in to the cdylib:

```bash
cargo build -p mcpg-plugin-transform-xml --features cdylib-export --release   # → target/release/libmcpg_plugin_transform_xml.so
```

## Sign & load (production)
Sign the artifact, pin/verify via the entry's `signature:` block, and honour
revocations. See <https://mcpg.dev/docs/security/plugin-security>.

## See also
- Plugin classes and the ABI: <https://mcpg.dev/docs/plugins/plugins-and-protocol>
- Pipeline step kinds, including `plugin_transform`: <https://mcpg.dev/docs/reference/pipeline-steps>
- Sibling transforms: `libs/plugins/transform/xslt` (XSLT stylesheets),
  `libs/plugins/transform/jsonata` (JSONata expressions),
  `libs/plugins/transform/csv` (CSV ⇄ JSON)
