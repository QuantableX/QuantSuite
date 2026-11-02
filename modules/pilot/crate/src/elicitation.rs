//! MCP elicitation (parity card 05): a server asks the user for structured
//! input (a form) or to finish something on a web page (URL mode).
//!
//! Both Codex (`mcpServer/elicitation/request`) and Claude Code
//! (`control_request` `elicitation`) pass the MCP request through. This
//! module turns the server's `requestedSchema` into a bounded, ordered field
//! list the UI can render, rejects what it cannot render faithfully (a
//! precise error goes back to the provider instead of a form that would
//! accept invalid answers), and checks the user's answer against the same
//! fields before anything is sent.
//!
//! Forms carry no secrets by the MCP rules (servers must use URL mode for
//! credentials); secret questions are `requestUserInput`'s `isSecret`, which
//! the runtime redacts from the saved timeline.
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fmt;
use tauri::Url;

const FIELD_LIMIT: usize = 32;
const OPTION_LIMIT: usize = 64;
const TEXT_LIMIT: usize = 8192;
const LABEL_LIMIT: usize = 512;
const MESSAGE_LIMIT: usize = 8192;
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

/// Annotations that change nothing about valid answers.
const ANNOTATIONS: &[&str] = &["$comment", "examples", "deprecated"];

fn text<'a>(v: Option<&'a Value>, what: &str, limit: usize) -> Result<Option<&'a str>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.len() <= limit => Ok(Some(s)),
        Some(Value::String(_)) => Err(format!("{what} is longer than {limit} bytes")),
        Some(_) => Err(format!("{what} must be text")),
    }
}

fn count(v: Option<&Value>, what: &str, limit: u64) -> Result<Option<u64>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(n) => n.as_u64().filter(|n| *n <= limit).map(Some).ok_or_else(|| format!("{what} must be a whole number up to {limit}")),
    }
}

fn number(v: Option<&Value>, what: &str) -> Result<Option<f64>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(n) => n.as_f64().filter(|n| n.is_finite() && n.abs() <= MAX_SAFE).map(Some).ok_or_else(|| format!("{what} must be a finite number")),
    }
}

fn check_keys(field: &str, obj: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    for k in obj.keys() {
        if !allowed.contains(&k.as_str()) && !ANNOTATIONS.contains(&k.as_str()) && !k.starts_with("x-") {
            return Err(format!("field \u{201c}{field}\u{201d} uses the unsupported schema keyword \u{201c}{k}\u{201d}"));
        }
    }
    Ok(())
}

fn valid_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_control) || ["__proto__", "prototype", "constructor"].contains(&key) {
        return Err(format!("the field name \u{201c}{}\u{201d} is not supported", key.chars().take(40).collect::<String>()));
    }
    Ok(())
}

/// `[{ value, label }]` from `enum` (+ legacy `enumNames`) or `oneOf`/`anyOf` `{ const, title }`.
fn options(field: &str, obj: &Map<String, Value>, consts: Option<&Value>) -> Result<Option<Vec<Value>>, String> {
    let mut out = Vec::new();
    if let Some(list) = consts {
        let list = list.as_array().ok_or_else(|| format!("field \u{201c}{field}\u{201d} has invalid titled options"))?;
        for o in list {
            let o = o.as_object().ok_or_else(|| format!("field \u{201c}{field}\u{201d} has invalid titled options"))?;
            check_keys(field, o, &["const", "title", "description"])?;
            let value = text(o.get("const"), "an option value", LABEL_LIMIT)?.ok_or_else(|| format!("field \u{201c}{field}\u{201d} has an option without a value"))?;
            let label = text(o.get("title"), "an option title", LABEL_LIMIT)?.unwrap_or(value);
            out.push(json!({ "value": value, "label": label }));
        }
    } else if let Some(list) = obj.get("enum") {
        let list = list.as_array().ok_or_else(|| format!("field \u{201c}{field}\u{201d} has an invalid option list"))?;
        let names = match obj.get("enumNames") {
            None | Some(Value::Null) => None,
            Some(Value::Array(n)) if n.len() == list.len() => Some(n),
            Some(_) => return Err(format!("field \u{201c}{field}\u{201d} has option labels that do not match its options")),
        };
        for (i, v) in list.iter().enumerate() {
            let value = v.as_str().filter(|s| s.len() <= LABEL_LIMIT).ok_or_else(|| format!("field \u{201c}{field}\u{201d} has a non-text option"))?;
            let label = match names {
                Some(n) => n[i].as_str().filter(|s| s.len() <= LABEL_LIMIT).ok_or_else(|| format!("field \u{201c}{field}\u{201d} has an invalid option label"))?,
                None => value,
            };
            out.push(json!({ "value": value, "label": label }));
        }
    } else {
        if obj.contains_key("enumNames") {
            return Err(format!("field \u{201c}{field}\u{201d} has option labels without options"));
        }
        return Ok(None);
    }
    if out.is_empty() || out.len() > OPTION_LIMIT {
        return Err(format!("field \u{201c}{field}\u{201d} needs 1 to {OPTION_LIMIT} options"));
    }
    let mut seen = HashSet::new();
    if !out.iter().all(|o| seen.insert(o["value"].as_str().unwrap_or_default().to_string())) {
        return Err(format!("field \u{201c}{field}\u{201d} repeats an option"));
    }
    Ok(Some(out))
}

fn field(key: &str, schema: &Value, required: bool) -> Result<Value, String> {
    valid_key(key)?;
    let obj = schema.as_object().ok_or_else(|| format!("field \u{201c}{key}\u{201d} is not a schema object"))?;
    let kind = text(obj.get("type"), "a field type", 16)?.unwrap_or("string");
    let title = text(obj.get("title"), "a field title", LABEL_LIMIT)?.unwrap_or(key);
    let description = text(obj.get("description"), "a field description", TEXT_LIMIT)?;
    let mut f = json!({ "key": key, "title": title, "description": description, "required": required });
    match kind {
        "string" => {
            check_keys(key, obj, &["type", "title", "description", "default", "minLength", "maxLength", "format", "enum", "enumNames", "oneOf"])?;
            if obj.contains_key("enum") && obj.contains_key("oneOf") {
                return Err(format!("field \u{201c}{key}\u{201d} declares its options twice"));
            }
            match options(key, obj, obj.get("oneOf"))? {
                Some(opts) => {
                    if ["minLength", "maxLength", "format"].iter().any(|k| obj.contains_key(*k)) {
                        return Err(format!("field \u{201c}{key}\u{201d} combines options with text constraints"));
                    }
                    f["kind"] = "choice".into();
                    f["options"] = opts.into();
                }
                None => {
                    f["kind"] = "text".into();
                    let min = count(obj.get("minLength"), "minLength", TEXT_LIMIT as u64)?;
                    let max = count(obj.get("maxLength"), "maxLength", TEXT_LIMIT as u64)?;
                    if let (Some(a), Some(b)) = (min, max) {
                        if a > b {
                            return Err(format!("field \u{201c}{key}\u{201d} has minLength above maxLength"));
                        }
                    }
                    f["minLength"] = json!(min);
                    f["maxLength"] = json!(max);
                    match text(obj.get("format"), "a format", 32)? {
                        None => {}
                        Some(fmt @ ("email" | "uri" | "date" | "date-time")) => f["format"] = fmt.into(),
                        Some(other) => return Err(format!("field \u{201c}{key}\u{201d} uses the unsupported format \u{201c}{other}\u{201d}")),
                    }
                }
            }
        }
        "number" | "integer" => {
            check_keys(key, obj, &["type", "title", "description", "default", "minimum", "maximum"])?;
            let min = number(obj.get("minimum"), "minimum")?;
            let max = number(obj.get("maximum"), "maximum")?;
            if let (Some(a), Some(b)) = (min, max) {
                if a > b {
                    return Err(format!("field \u{201c}{key}\u{201d} has minimum above maximum"));
                }
            }
            f["kind"] = kind.into();
            f["minimum"] = json!(min);
            f["maximum"] = json!(max);
        }
        "boolean" => {
            check_keys(key, obj, &["type", "title", "description", "default"])?;
            f["kind"] = "boolean".into();
        }
        "array" => {
            check_keys(key, obj, &["type", "title", "description", "default", "items", "minItems", "maxItems", "uniqueItems"])?;
            let items = obj.get("items").and_then(Value::as_object).ok_or_else(|| format!("field \u{201c}{key}\u{201d} is a list without item options"))?;
            if items.contains_key("type") && items.get("type") != Some(&json!("string")) {
                return Err(format!("field \u{201c}{key}\u{201d} lists something other than text options"));
            }
            check_keys(key, items, &["type", "enum", "enumNames", "anyOf", "oneOf"])?;
            let consts = items.get("anyOf").or_else(|| items.get("oneOf"));
            let opts = options(key, items, consts)?.ok_or_else(|| format!("field \u{201c}{key}\u{201d} is a list without item options"))?;
            let min = count(obj.get("minItems"), "minItems", OPTION_LIMIT as u64)?;
            let max = count(obj.get("maxItems"), "maxItems", OPTION_LIMIT as u64)?;
            if let (Some(a), Some(b)) = (min, max) {
                if a > b {
                    return Err(format!("field \u{201c}{key}\u{201d} has minItems above maxItems"));
                }
            }
            f["kind"] = "multi".into();
            f["options"] = opts.into();
            f["minItems"] = json!(min);
            f["maxItems"] = json!(max);
        }
        other => return Err(format!("field \u{201c}{key}\u{201d} has the unsupported type \u{201c}{other}\u{201d}")),
    }
    if let Some(d) = obj.get("default").filter(|d| !d.is_null()) {
        // A default the field itself would refuse is the server's mistake;
        // the form starts empty instead of pre-filling an invalid answer.
        if check_value(&f, d).is_ok() {
            f["default"] = d.clone();
        }
    }
    Ok(f)
}

/// The server's `requestedSchema` as an ordered field list. `order` is the
/// property order as the server wrote it (JSON objects lose it here).
pub fn form(schema: &Value, order: Option<&[String]>) -> Result<Vec<Value>, String> {
    let obj = schema.as_object().ok_or("the form schema is not an object")?;
    for k in obj.keys() {
        if !["$schema", "type", "title", "description", "properties", "required", "additionalProperties"].contains(&k.as_str())
            && !ANNOTATIONS.contains(&k.as_str())
        {
            return Err(format!("the form uses the unsupported schema keyword \u{201c}{k}\u{201d}"));
        }
    }
    if obj.get("type").is_some_and(|t| t != "object") {
        return Err("only flat object forms are supported".into());
    }
    if obj.get("additionalProperties").is_some_and(|a| a != &json!(false)) {
        return Err("forms with arbitrary extra fields are not supported".into());
    }
    let empty = Map::new();
    let props = match obj.get("properties") {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(p)) => p,
        Some(_) => return Err("the form's properties are not an object".into()),
    };
    if props.len() > FIELD_LIMIT {
        return Err(format!("forms support at most {FIELD_LIMIT} fields"));
    }
    let required: Vec<&str> = match obj.get("required") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(r)) => r.iter().map(|v| v.as_str().ok_or("the form's required list is invalid")).collect::<Result<_, _>>()?,
        Some(_) => return Err("the form's required list is invalid".into()),
    };
    if let Some(unknown) = required.iter().find(|r| !props.contains_key(**r)) {
        return Err(format!("the form requires the unknown field \u{201c}{unknown}\u{201d}"));
    }
    let mut keys: Vec<&String> = Vec::with_capacity(props.len());
    for k in order.unwrap_or_default() {
        if let Some((key, _)) = props.get_key_value(k) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    for k in props.keys() {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys.into_iter().map(|k| field(k, &props[k], required.contains(&k.as_str()))).collect()
}

fn check_format(format: &str, s: &str) -> bool {
    let digits = |p: &str| p.chars().all(|c| c.is_ascii_digit());
    let date = |d: &str| {
        let p: Vec<&str> = d.split('-').collect();
        p.len() == 3
            && p[0].len() == 4
            && p[1].len() == 2
            && p[2].len() == 2
            && p.iter().all(|x| digits(x))
            && (1..=12).contains(&p[1].parse::<u32>().unwrap_or(0))
            && (1..=31).contains(&p[2].parse::<u32>().unwrap_or(0))
    };
    match format {
        "email" => {
            let mut parts = s.splitn(2, '@');
            let (local, domain) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !s.chars().any(char::is_whitespace)
        }
        "uri" => Url::parse(s).is_ok(),
        "date" => date(s),
        "date-time" => chrono::DateTime::parse_from_rfc3339(s).is_ok(),
        _ => true,
    }
}

/// Whether `value` is a valid answer for the normalized field `f`.
fn check_value(f: &Value, value: &Value) -> Result<(), String> {
    let title = f["title"].as_str().unwrap_or_default();
    let bad = |why: &str| Err(format!("\u{201c}{title}\u{201d} {why}"));
    let in_options = |v: &str| f["options"].as_array().is_some_and(|o| o.iter().any(|o| o["value"] == v));
    match f["kind"].as_str().unwrap_or_default() {
        "text" => {
            let Some(s) = value.as_str() else { return bad("must be text") };
            let len = s.chars().count() as u64;
            if s.len() > TEXT_LIMIT {
                return bad("is too long");
            }
            if f["minLength"].as_u64().is_some_and(|m| len < m) {
                return bad(&format!("needs at least {} characters", f["minLength"]));
            }
            if f["maxLength"].as_u64().is_some_and(|m| len > m) {
                return bad(&format!("allows at most {} characters", f["maxLength"]));
            }
            if let Some(fmt) = f["format"].as_str() {
                if !check_format(fmt, s) {
                    return bad(&format!("is not a valid {}", fmt.replace('-', " ")));
                }
            }
        }
        "choice" => {
            if !value.as_str().is_some_and(in_options) {
                return bad("must be one of the offered options");
            }
        }
        "multi" => {
            let Some(list) = value.as_array() else { return bad("must be a list of options") };
            let mut seen = HashSet::new();
            for v in list {
                let Some(s) = v.as_str().filter(|s| in_options(s)) else { return bad("contains an option that was not offered") };
                if !seen.insert(s) {
                    return bad("repeats an option");
                }
            }
            let n = list.len() as u64;
            if f["minItems"].as_u64().is_some_and(|m| n < m) {
                return bad(&format!("needs at least {} choices", f["minItems"]));
            }
            if f["maxItems"].as_u64().is_some_and(|m| n > m) {
                return bad(&format!("allows at most {} choices", f["maxItems"]));
            }
        }
        "number" | "integer" => {
            let Some(n) = value.as_f64().filter(|n| n.is_finite() && n.abs() <= MAX_SAFE) else { return bad("must be a number") };
            if f["kind"] == "integer" && n.fract() != 0.0 {
                return bad("must be a whole number");
            }
            if f["minimum"].as_f64().is_some_and(|m| n < m) {
                return bad(&format!("must be at least {}", f["minimum"]));
            }
            if f["maximum"].as_f64().is_some_and(|m| n > m) {
                return bad(&format!("must be at most {}", f["maximum"]));
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                return bad("must be yes or no");
            }
        }
        _ => return bad("has an unknown type"),
    }
    Ok(())
}

/// The user's form answer, checked against the fields: what is sent as the
/// elicitation result's `content`. Blank optional fields are left out;
/// integers go out as integers.
pub fn content(fields: &[Value], answer: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    if let Some(unknown) = answer.keys().find(|k| !fields.iter().any(|f| f["key"] == k.as_str())) {
        return Err(format!("the answer has the unknown field \u{201c}{unknown}\u{201d}"));
    }
    let mut out = Map::new();
    for f in fields {
        let key = f["key"].as_str().unwrap_or_default();
        let value = answer.get(key).filter(|v| !v.is_null() && v.as_str() != Some("") && v.as_array().map_or(true, |a| !a.is_empty()));
        let Some(value) = value else {
            if f["required"] == true {
                return Err(format!("\u{201c}{}\u{201d} is required", f["title"].as_str().unwrap_or(key)));
            }
            continue;
        };
        check_value(f, value)?;
        let value = match (f["kind"].as_str(), value.as_f64()) {
            (Some("integer"), Some(n)) => json!(n as i64),
            _ => value.clone(),
        };
        out.insert(key.to_string(), value);
    }
    Ok(out)
}

/// A URL-mode destination: http(s) with a host, no credentials, nothing hidden.
pub fn url(raw: &str) -> Result<(String, String), String> {
    if raw.len() > TEXT_LIMIT || raw.chars().any(|c| c.is_control() || c.is_whitespace()) || raw.contains('\\') {
        return Err("the web address is malformed".into());
    }
    let parsed = Url::parse(raw).map_err(|_| "the web address is invalid".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only http and https addresses can be opened".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("web addresses with embedded credentials are refused".into());
    }
    let host = parsed.host_str().filter(|h| !h.is_empty()).ok_or("the web address has no host")?.to_string();
    Ok((raw.to_string(), host))
}

/// What the UI shows for an elicitation: the request item's `input`.
pub fn request_input(server: &str, message: &str, mode: &str, url_or_fields: Result<(String, String), Vec<Value>>, id: Option<&str>) -> Value {
    let message: String = message.chars().take(MESSAGE_LIMIT).collect();
    match url_or_fields {
        Ok((url, host)) => json!({ "mode": "url", "server": server, "message": message, "url": url, "host": host, "elicitationId": id }),
        Err(fields) => json!({ "mode": mode, "server": server, "message": message, "fields": fields }),
    }
}

/// The normalized fields of a request input.
pub fn fields_of(input: &Value) -> Vec<Value> {
    input["fields"].as_array().cloned().unwrap_or_default()
}

/// The elicitation result for the provider: `{ action, content? }`.
pub fn result(input: &Value, action: &str, answer: Option<&Map<String, Value>>) -> Result<Value, String> {
    match action {
        "decline" | "cancel" => Ok(json!({ "action": action })),
        "accept" if input["mode"] == "url" => Ok(json!({ "action": "accept" })),
        "accept" => Ok(json!({ "action": "accept", "content": content(&fields_of(input), answer.unwrap_or(&Map::new()))? })),
        _ => Err("choose accept, decline or cancel".into()),
    }
}

/// The property order of `params.requestedSchema.properties` in a raw
/// JSON-RPC line — `serde_json::Value` sorts object keys.
pub fn property_order(line: &str) -> Option<Vec<String>> {
    struct Keys(Vec<String>);
    impl<'de> Deserialize<'de> for Keys {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Keys;
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("an object")
                }
                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                    let mut keys = Vec::new();
                    while let Some(k) = map.next_key::<String>()? {
                        map.next_value::<IgnoredAny>()?;
                        keys.push(k);
                    }
                    Ok(Keys(keys))
                }
            }
            d.deserialize_map(V)
        }
    }
    #[derive(Deserialize)]
    struct Schema {
        properties: Option<Keys>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Params {
        requested_schema: Option<Schema>,
    }
    #[derive(Deserialize)]
    struct Line {
        params: Option<Params>,
    }
    serde_json::from_str::<Line>(line).ok()?.params?.requested_schema?.properties.map(|k| k.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "title": "Name", "minLength": 2, "maxLength": 20 },
                "count": { "type": "integer", "minimum": 1, "maximum": 5, "default": 2 },
                "ratio": { "type": "number" },
                "enabled": { "type": "boolean", "default": true },
                "region": { "type": "string", "enum": ["east", "west"], "enumNames": ["East", "West"] },
                "tier": { "type": "string", "oneOf": [{ "const": "a", "title": "Tier A" }, { "const": "b", "title": "Tier B" }] },
                "tags": { "type": "array", "items": { "type": "string", "enum": ["x", "y", "z"] }, "minItems": 1, "maxItems": 2 },
                "mail": { "type": "string", "format": "email" }
            },
            "required": ["name", "count", "region"]
        })
    }

    fn f() -> Vec<Value> {
        form(&schema(), Some(&["name".into(), "count".into(), "region".into()])).unwrap()
    }

    #[test]
    fn fields_keep_server_order_types_options_and_required() {
        let fields = f();
        let keys: Vec<&str> = fields.iter().map(|f| f["key"].as_str().unwrap()).collect();
        assert_eq!(&keys[..3], ["name", "count", "region"], "the server's order comes first");
        assert_eq!(keys.len(), 8);
        let get = |k: &str| fields.iter().find(|f| f["key"] == k).unwrap().clone();
        assert_eq!(get("name")["required"], true);
        assert_eq!(get("ratio")["required"], false);
        assert_eq!(get("count")["kind"], "integer");
        assert_eq!(get("count")["default"], 2);
        assert_eq!(get("region")["options"][1], json!({ "value": "west", "label": "West" }));
        assert_eq!(get("tier")["kind"], "choice");
        assert_eq!(get("tier")["options"][0]["label"], "Tier A");
        assert_eq!(get("tags")["kind"], "multi");
        assert_eq!(get("mail")["format"], "email");
    }

    #[test]
    fn unsupported_constraints_are_refused_precisely() {
        let bad = |prop: Value| form(&json!({ "type": "object", "properties": { "p": prop } }), None).unwrap_err();
        assert!(bad(json!({ "type": "string", "pattern": "^a" })).contains("pattern"));
        assert!(bad(json!({ "type": "object" })).contains("unsupported type"));
        assert!(bad(json!({ "type": "string", "format": "ipv4" })).contains("ipv4"));
        assert!(bad(json!({ "type": "integer", "minimum": 5, "maximum": 1 })).contains("minimum above maximum"));
        assert!(bad(json!({ "type": "string", "enum": ["a", "a"] })).contains("repeats"));
        assert!(form(&json!({ "type": "object", "properties": {}, "required": ["ghost"] }), None).unwrap_err().contains("ghost"));
        assert!(form(&json!({ "type": "object", "properties": {}, "additionalProperties": true }), None).is_err());
        // Pure annotations are fine.
        assert!(form(&json!({ "type": "object", "properties": { "p": { "type": "string", "examples": ["x"], "$comment": "c" } } }), None).is_ok());
    }

    #[test]
    fn answers_are_checked_and_optional_blanks_left_out() {
        let fields = f();
        let ok = content(
            &fields,
            json!({ "name": "Ada", "count": 3.0, "region": "east", "ratio": null, "mail": "", "tags": ["x"], "enabled": false })
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(ok["count"], json!(3), "integers go out as integers");
        assert!(!ok.contains_key("ratio") && !ok.contains_key("mail"), "blank optional fields are omitted");
        assert_eq!(ok["enabled"], false);

        let err = |a: Value| content(&fields, a.as_object().unwrap()).unwrap_err();
        assert!(err(json!({ "count": 3, "region": "east" })).contains("Name"), "required");
        assert!(err(json!({ "name": "A", "count": 3, "region": "east" })).contains("at least 2"));
        assert!(err(json!({ "name": "Ada", "count": 9, "region": "east" })).contains("at most 5"));
        assert!(err(json!({ "name": "Ada", "count": 2.5, "region": "east" })).contains("whole number"));
        assert!(err(json!({ "name": "Ada", "count": 2, "region": "north" })).contains("offered"));
        assert!(err(json!({ "name": "Ada", "count": 2, "region": "east", "tags": ["x", "y", "z"] })).contains("at most 2"));
        assert!(err(json!({ "name": "Ada", "count": 2, "region": "east", "mail": "nope" })).contains("email"));
        assert!(err(json!({ "name": "Ada", "count": 2, "region": "east", "extra": 1 })).contains("unknown"));
    }

    #[test]
    fn url_mode_accepts_only_plain_http_destinations() {
        assert_eq!(url("https://example.com/connect?state=x").unwrap().1, "example.com");
        for bad in ["javascript:alert(1)", "file:///c:/x", "https://user:pw@example.com", "https://exa mple.com", "not a url", "https://a.com\\b"] {
            assert!(url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn results_follow_the_action() {
        let input = request_input("srv", "Configure", "form", Err(f()), None);
        assert_eq!(result(&input, "decline", None).unwrap(), json!({ "action": "decline" }));
        assert_eq!(result(&input, "cancel", None).unwrap(), json!({ "action": "cancel" }));
        assert!(result(&input, "accept", None).is_err(), "required fields missing");
        let url_input = request_input("srv", "Sign in", "url", Ok(url("https://example.com").unwrap()), Some("e1"));
        assert_eq!(result(&url_input, "accept", None).unwrap(), json!({ "action": "accept" }));
        assert!(result(&url_input, "maybe", None).is_err());
    }

    #[test]
    fn property_order_reads_the_raw_line() {
        let line = r#"{"id":1,"method":"m","params":{"requestedSchema":{"type":"object","properties":{"zeta":{},"alpha":{},"mid":{}}}}}"#;
        assert_eq!(property_order(line).unwrap(), ["zeta", "alpha", "mid"]);
        assert!(property_order(r#"{"id":1}"#).is_none());
    }
}
