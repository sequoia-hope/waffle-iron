//! Script interface header (`specs/custom_features_and_modeling_roadmap.md`
//! §A5): the leading comment block a script declares its parameters in.
//!
//! ```text
//! // @feature name="Spur gear" version=1
//! // @param tooth_count: int = 24  min=6  max=400
//! // @param module: length = 0.002
//! // @param plane: plane
//! // @output body: main
//! ```
//!
//! Parsed BEFORE evaluation; drives argument typing/defaults (and, later,
//! the feature dialog and the MCP input schema). Only the leading run of
//! comment/blank lines is scanned; the first code line ends the header.

use std::collections::BTreeMap;

use serde::Serialize;

/// The type of a declared `@param`. Serializes as its header spelling
/// (`"int"`, `"length"`, …) — what the dialog generator and the
/// `script_run_check` tool hand to hosts (A-M4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    /// An integer (script sees `INT`).
    Int,
    /// A plain number (script sees `FLOAT`).
    Number,
    /// A length in meters (an expression drives it in mm-space).
    Length,
    /// An angle in degrees.
    Angle,
    Bool,
    String,
    /// A sketch plane: `{origin, normal}`, a datum plane id, or a face query.
    Plane,
    /// A body of the tree OUTSIDE the script (a `GeomRef` of kind `Solid`):
    /// the script may target it (`combine: "Cut", targets: [p.target]`),
    /// consuming it like any boolean would.
    Body,
    /// A face outside the script (a `GeomRef` of kind `Face`): a sketch
    /// plane, a connector pick, or a query base.
    Face,
    /// An edge outside the script (a `GeomRef` of kind `Edge`).
    Edge,
}

impl ParamType {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "int" => ParamType::Int,
            "number" | "float" => ParamType::Number,
            "length" => ParamType::Length,
            "angle" => ParamType::Angle,
            "bool" => ParamType::Bool,
            "string" => ParamType::String,
            "plane" => ParamType::Plane,
            "body" => ParamType::Body,
            "face" => ParamType::Face,
            "edge" => ParamType::Edge,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            ParamType::Int => "int",
            ParamType::Number => "number",
            ParamType::Length => "length",
            ParamType::Angle => "angle",
            ParamType::Bool => "bool",
            ParamType::String => "string",
            ParamType::Plane => "plane",
            ParamType::Body => "body",
            ParamType::Face => "face",
            ParamType::Edge => "edge",
        }
    }

    /// Geometry-valued parameters (a `GeomRef` argument, no literal default).
    pub fn is_geometry(self) -> bool {
        matches!(
            self,
            ParamType::Plane | ParamType::Body | ParamType::Face | ParamType::Edge
        )
    }
}

/// A literal default value from the header.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Literal {
    Number(f64),
    Bool(bool),
    Text(String),
}

/// One `@param` declaration.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: ParamType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// What kind of thing a declared `@output` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputKind {
    /// The node's primary body (`OutputKey::Main`).
    Main,
    /// A secondary body (`OutputKey::Named { name }`).
    Body,
    /// A face (`Role::Named { name }` on the resolved face).
    Face,
    /// An edge (`Role::Named { name }` on the resolved edge).
    Edge,
    /// A mate connector the script places (`ctx.mate_connector`).
    Connector,
}

impl OutputKind {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "main" => OutputKind::Main,
            "body" => OutputKind::Body,
            "face" => OutputKind::Face,
            "edge" => OutputKind::Edge,
            "connector" => OutputKind::Connector,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            OutputKind::Main => "main",
            OutputKind::Body => "body",
            OutputKind::Face => "face",
            OutputKind::Edge => "edge",
            OutputKind::Connector => "connector",
        }
    }
}

/// One `@output name: kind` declaration. A declared output is a CONTRACT:
/// the script's return value must provide it (a `connector` is provided by
/// `ctx.mate_connector(#{ name })`), and its kind must match.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutputDecl {
    pub name: String,
    pub kind: OutputKind,
}

/// The parsed header.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ScriptInterface {
    pub name: String,
    pub version: u32,
    pub params: Vec<ParamDecl>,
    /// `@output` declarations, in order. A script with none may still return
    /// named outputs; declaring them documents the interface and makes a
    /// missing one loud.
    pub outputs: Vec<OutputDecl>,
}

impl ScriptInterface {
    pub fn param(&self, name: &str) -> Option<&ParamDecl> {
        self.params.iter().find(|p| p.name == name)
    }

    pub fn output(&self, name: &str) -> Option<&OutputDecl> {
        self.outputs.iter().find(|o| o.name == name)
    }
}

/// Tokenize `key=value` pairs where a value may be a quoted string.
fn key_values(rest: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut chars = rest.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        let Some(&c) = chars.peek() else { break };
        if !(c.is_alphanumeric() || c == '_') {
            return Err(format!("unexpected `{c}` in header attributes"));
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                key.push(c);
                chars.next();
            } else {
                break;
            }
        }
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() != Some(&'=') {
            return Err(format!("attribute `{key}` has no `=value`"));
        }
        chars.next();
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some(c) => value.push(c),
                    None => return Err(format!("unterminated string for `{key}`")),
                }
            }
            value = format!("\"{value}\"");
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        out.insert(key, value);
    }
    Ok(out)
}

fn parse_literal(s: &str) -> Result<Literal, String> {
    if let Some(inner) = s.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return Ok(Literal::Text(inner.to_string()));
    }
    match s {
        "true" => return Ok(Literal::Bool(true)),
        "false" => return Ok(Literal::Bool(false)),
        _ => {}
    }
    s.parse::<f64>()
        .map(Literal::Number)
        .map_err(|_| format!("`{s}` is not a number, `true`/`false`, or a quoted string"))
}

/// Parse the header of `text`.
pub fn parse_header(text: &str) -> Result<ScriptInterface, String> {
    let mut iface = ScriptInterface::default();
    let mut seen_feature = false;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some(comment) = line.strip_prefix("//") else {
            break; // first code line ends the header
        };
        let comment = comment.trim();
        let Some(directive) = comment.strip_prefix('@') else {
            continue; // an ordinary comment
        };
        let (tag, rest) = directive
            .split_once(char::is_whitespace)
            .unwrap_or((directive, ""));
        let at = |msg: String| format!("line {}: {msg}", lineno + 1);
        match tag {
            "feature" => {
                let kv = key_values(rest).map_err(at)?;
                if let Some(n) = kv.get("name") {
                    iface.name = n.trim_matches('"').to_string();
                }
                if let Some(v) = kv.get("version") {
                    iface.version = v
                        .parse()
                        .map_err(|_| at(format!("version `{v}` is not an integer")))?;
                }
                seen_feature = true;
            }
            "param" => {
                // name: type [= default] [min=..] [max=..]
                let (name, after) = rest
                    .split_once(':')
                    .ok_or_else(|| at("@param needs `name: type`".into()))?;
                let name = name.trim().to_string();
                if name.is_empty()
                    || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || name.chars().next().is_some_and(|c| c.is_ascii_digit())
                {
                    return Err(at(format!("`{name}` is not a valid parameter name")));
                }
                if iface.params.iter().any(|p| p.name == name) {
                    return Err(at(format!("parameter `{name}` declared twice")));
                }
                let after = after.trim();
                let (ty_s, tail) = after
                    .split_once(|c: char| c.is_whitespace() || c == '=')
                    .map(|(a, _)| (a, &after[a.len()..]))
                    .unwrap_or((after, ""));
                let ty = ParamType::parse(ty_s.trim())
                    .ok_or_else(|| at(format!("unknown parameter type `{}`", ty_s.trim())))?;
                let mut tail = tail.trim();
                let mut default = None;
                if let Some(t) = tail.strip_prefix('=') {
                    let t = t.trim_start();
                    // The default runs to the next whitespace (or is a quoted string).
                    let (lit, rest2) = if let Some(body) = t.strip_prefix('"') {
                        let end = body
                            .find('"')
                            .ok_or_else(|| at("unterminated default string".into()))?;
                        (&t[..end + 2], &t[end + 2..])
                    } else {
                        t.split_once(char::is_whitespace).unwrap_or((t, ""))
                    };
                    default = Some(parse_literal(lit).map_err(at)?);
                    tail = rest2.trim();
                }
                let kv = key_values(tail).map_err(at)?;
                let num = |k: &str| -> Result<Option<f64>, String> {
                    kv.get(k)
                        .map(|v| {
                            v.parse::<f64>()
                                .map_err(|_| at(format!("{k}=`{v}` is not a number")))
                        })
                        .transpose()
                };
                let decl = ParamDecl {
                    name,
                    ty,
                    default,
                    min: num("min")?,
                    max: num("max")?,
                };
                let default_matches = match (&decl.default, decl.ty) {
                    (None, _) => true,
                    (Some(Literal::Number(_)), t) => {
                        !matches!(t, ParamType::Bool | ParamType::String) && !t.is_geometry()
                    }
                    (Some(Literal::Bool(_)), t) => t == ParamType::Bool,
                    (Some(Literal::Text(_)), t) => t == ParamType::String,
                };
                if !default_matches {
                    return Err(at(format!(
                        "default of `{}` does not match its type {}",
                        decl.name,
                        decl.ty.label()
                    )));
                }
                iface.params.push(decl);
            }
            "output" => {
                // name: kind   (kind ∈ main | body | face | edge | connector)
                let (name, kind_s) = rest
                    .split_once(':')
                    .ok_or_else(|| at("@output needs `name: kind`".into()))?;
                let name = name.trim().to_string();
                if name.is_empty()
                    || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || name.chars().next().is_some_and(|c| c.is_ascii_digit())
                {
                    return Err(at(format!("`{name}` is not a valid output name")));
                }
                if iface.outputs.iter().any(|o| o.name == name) {
                    return Err(at(format!("output `{name}` declared twice")));
                }
                let kind_s = kind_s
                    .trim()
                    .split(char::is_whitespace)
                    .next()
                    .unwrap_or("");
                let kind = OutputKind::parse(kind_s).ok_or_else(|| {
                    at(format!(
                        "unknown output kind `{kind_s}` (main, body, face, edge, connector)"
                    ))
                })?;
                if kind == OutputKind::Main && iface.outputs.iter().any(|o| o.kind == kind) {
                    return Err(at("only one output can be `main`".into()));
                }
                iface.outputs.push(OutputDecl { name, kind });
            }
            other => return Err(at(format!("unknown header directive `@{other}`"))),
        }
    }
    if !seen_feature {
        return Err("no `// @feature name=\"…\"` header line".into());
    }
    Ok(iface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_spec_example() {
        let text = r#"
// @feature name="Spur gear" version=1
// @param tooth_count: int = 24  min=6  max=400
// @param module_m:    length = 0.002
// @param pressure_angle_deg: angle = 20
// @param face_width: length = 0.010
// @param plane: plane
// @output body: main

fn feature(ctx, p) { }
"#;
        let h = parse_header(text).unwrap();
        assert_eq!(h.name, "Spur gear");
        assert_eq!(h.version, 1);
        assert_eq!(h.params.len(), 5);
        let t = h.param("tooth_count").unwrap();
        assert_eq!(t.ty, ParamType::Int);
        assert_eq!(t.default, Some(Literal::Number(24.0)));
        assert_eq!((t.min, t.max), (Some(6.0), Some(400.0)));
        assert_eq!(h.param("plane").unwrap().ty, ParamType::Plane);
        assert_eq!(
            h.outputs,
            vec![OutputDecl {
                name: "body".into(),
                kind: OutputKind::Main
            }]
        );
    }

    #[test]
    fn output_declarations_are_typed_and_unique() {
        let ok = parse_header(
            "// @feature name=\"x\"\n// @output body: main\n// @output top: face\n// @output pin: connector\n",
        )
        .unwrap();
        assert_eq!(ok.outputs.len(), 3);
        assert_eq!(ok.output("top").unwrap().kind, OutputKind::Face);
        assert!(parse_header("// @feature name=\"x\"\n// @output body\n").is_err());
        assert!(parse_header("// @feature name=\"x\"\n// @output body: solid\n").is_err());
        assert!(
            parse_header("// @feature name=\"x\"\n// @output a: face\n// @output a: edge\n")
                .is_err()
        );
        assert!(
            parse_header("// @feature name=\"x\"\n// @output a: main\n// @output b: main\n")
                .is_err()
        );
    }

    #[test]
    fn rejects_bad_headers() {
        assert!(parse_header("fn feature(ctx, p) {}").is_err());
        assert!(parse_header("// @feature name=\"x\"\n// @param a: nope\n").is_err());
        assert!(parse_header("// @feature name=\"x\"\n// @param a: int = true\n").is_err());
        assert!(
            parse_header("// @feature name=\"x\"\n// @param a: int\n// @param a: int\n").is_err()
        );
        assert!(parse_header("// @feature name=\"x\"\n// @bogus\n").is_err());
    }
}
