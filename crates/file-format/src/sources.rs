//! The v4 `sources` table: external content a document depends on, with
//! git-aware locators (`specs/waffle_v4_document_model.md` §2.3–2.4).
//!
//! Every tagged enum here keeps an **`Unknown`** variant that preserves a
//! well-formed-but-unrecognized `{"type": …}` object verbatim (§2.5): a
//! reader that predates a source or locator kind keeps the entry, reports it
//! unresolvable, and re-emits it byte-for-byte. A *malformed* object (no
//! string `type`, or a known type whose fields do not parse) is still a hard
//! parse error — opacity is for the future, not for corruption.

use chrono::{DateTime, Utc};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::hash::git_blob_sha1;

pub(crate) use feature_engine::opaque::known_or_unknown;

/// Which API adapter resolves a `Git` locator (§7.2). Absent ⇒ inferred from
/// the remote's hostname, else `Generic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum GitHost {
    Github,
    Gitlab,
    Gitea,
    Generic,
}

impl GitHost {
    /// Best-effort inference from a clone URL's hostname.
    pub fn infer(remote: &str) -> GitHost {
        let host = remote
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if host == "github.com" || host.ends_with(".github.com") {
            GitHost::Github
        } else if host == "gitlab.com" || host.contains("gitlab") {
            GitHost::Gitlab
        } else if host.contains("gitea") || host.contains("forgejo") || host == "codeberg.org" {
            GitHost::Gitea
        } else {
            GitHost::Generic
        }
    }
}

/// A git ref: pinned to a commit, or floating on a branch/tag tip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum GitRef {
    /// Reproducible forever; "update" is a no-op.
    Commit { sha: String },
    /// Floating; `SourceEntry::resolved` records the tip at last sync.
    Branch { name: String },
    /// Floating (tags can move); UIs may label it "release".
    Tag { name: String },
}

impl GitRef {
    pub fn is_pinned(&self) -> bool {
        matches!(self, GitRef::Commit { .. })
    }

    fn validate(&self) -> Option<String> {
        match self {
            GitRef::Commit { sha } => {
                let ok = (sha.len() == 40 || sha.len() == 64)
                    && sha.bytes().all(|b| b.is_ascii_hexdigit());
                (!ok).then(|| format!("commit sha `{sha}` is not 40/64 hex digits"))
            }
            GitRef::Branch { name } | GitRef::Tag { name } => validate_ref_name(name),
        }
    }
}

/// git-check-ref-format, the parts that matter for a URL path: non-empty, no
/// `..`, no leading `-` or `/`, no control/space, none of `\ ~ ^ : ? * [`.
fn validate_ref_name(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("ref name is empty".into());
    }
    if name.contains("..") || name.starts_with('-') || name.starts_with('/') || name.ends_with('/')
    {
        return Some(format!("ref name `{name}` is not a valid git ref"));
    }
    if name
        .chars()
        .any(|c| c.is_control() || c == ' ' || "\\~^:?*[".contains(c))
    {
        return Some(format!(
            "ref name `{name}` contains a character git refuses"
        ));
    }
    None
}

fn validate_repo_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("path is empty".into());
    }
    if path.starts_with('/') {
        return Some(format!(
            "path `{path}` must be repository-relative (no leading `/`)"
        ));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Some(format!("path `{path}` must not contain `..` segments"));
    }
    if path.chars().any(|c| c.is_control()) {
        return Some(format!("path `{path}` contains control characters"));
    }
    None
}

fn validate_https(url: &str, what: &str) -> Option<String> {
    (!url.starts_with("https://")).then(|| format!("{what} `{url}` must be an https:// URL"))
}

/// Where a source's content lives (§2.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Locator {
    /// `path` in the repository at `remote`, at `ref`.
    Git {
        /// HTTPS clone URL, normalized without a trailing `.git`.
        remote: String,
        /// Repository-relative, `/`-separated, no leading `/`.
        path: String,
        #[serde(rename = "ref")]
        git_ref: GitRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<GitHost>,
    },
    /// Relative to the document's own location, same ref (§2.4).
    Relative { path: String },
    /// Plain HTTPS fetch; `content_hash` is the only staleness signal.
    Url { url: String },
    /// A document in one of this browser's storage providers. Not shareable.
    Local { provider: String, doc_id: String },
    /// The `embed` IS the source (file picker / paste, no origin).
    Embedded,
    /// A well-formed locator this reader does not know; preserved verbatim.
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum KnownLocator {
    Git {
        remote: String,
        path: String,
        #[serde(rename = "ref")]
        git_ref: GitRef,
        #[serde(default)]
        host: Option<GitHost>,
    },
    Relative {
        path: String,
    },
    Url {
        url: String,
    },
    Local {
        provider: String,
        doc_id: String,
    },
    Embedded,
}

const LOCATOR_TAGS: &[&str] = &["Git", "Relative", "Url", "Local", "Embedded"];

impl<'de> Deserialize<'de> for Locator {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(
            match known_or_unknown::<D, KnownLocator>(d, LOCATOR_TAGS, "locator")? {
                Ok(KnownLocator::Git {
                    remote,
                    path,
                    git_ref,
                    host,
                }) => Locator::Git {
                    remote: normalize_remote(&remote),
                    path,
                    git_ref,
                    host,
                },
                Ok(KnownLocator::Relative { path }) => Locator::Relative { path },
                Ok(KnownLocator::Url { url }) => Locator::Url { url },
                Ok(KnownLocator::Local { provider, doc_id }) => Locator::Local { provider, doc_id },
                Ok(KnownLocator::Embedded) => Locator::Embedded,
                Err(v) => Locator::Unknown(v),
            },
        )
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for Locator {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Locator".into()
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Where a source's content lives. Unknown `type`s are preserved verbatim and reported unresolvable.",
            "oneOf": [
                {
                    "type": "object", "required": ["type", "remote", "path", "ref"],
                    "properties": {
                        "type": { "const": "Git" },
                        "remote": { "type": "string", "description": "HTTPS clone URL, normalized without a trailing .git" },
                        "path": { "type": "string", "description": "Repository-relative, /-separated, no leading /" },
                        "ref": g.subschema_for::<GitRef>(),
                        "host": { "anyOf": [ g.subschema_for::<GitHost>(), { "type": "null" } ] }
                    }
                },
                { "type": "object", "required": ["type", "path"],
                  "properties": { "type": { "const": "Relative" }, "path": { "type": "string" } } },
                { "type": "object", "required": ["type", "url"],
                  "properties": { "type": { "const": "Url" }, "url": { "type": "string" } } },
                { "type": "object", "required": ["type", "provider", "doc_id"],
                  "properties": { "type": { "const": "Local" }, "provider": { "type": "string" }, "doc_id": { "type": "string" } } },
                { "type": "object", "required": ["type"], "properties": { "type": { "const": "Embedded" } } },
                { "type": "object", "description": "Unknown locator kind (opaque, preserved).", "required": ["type"],
                  "properties": { "type": { "type": "string", "not": { "enum": LOCATOR_TAGS } } } }
            ]
        })
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for SourceKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SourceKind".into()
    }
    fn json_schema(_g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "What a source is. Unknown `type`s are preserved verbatim.",
            "oneOf": [
                { "type": "object", "required": ["type"], "properties": { "type": { "enum": SOURCE_KIND_TAGS } } },
                { "type": "object", "description": "Unknown source kind (opaque, preserved).", "required": ["type"],
                  "properties": { "type": { "type": "string", "not": { "enum": SOURCE_KIND_TAGS } } } }
            ]
        })
    }
}

/// Strip a trailing `.git` and trailing `/` from a clone URL.
pub fn normalize_remote(remote: &str) -> String {
    let r = remote.trim_end_matches('/');
    r.strip_suffix(".git").unwrap_or(r).to_string()
}

impl Locator {
    /// Git locator on a branch tip.
    pub fn git_branch(remote: &str, path: &str, branch: &str) -> Self {
        Locator::Git {
            remote: normalize_remote(remote),
            path: path.to_string(),
            git_ref: GitRef::Branch {
                name: branch.to_string(),
            },
            host: None,
        }
    }

    /// Git locator pinned to a commit.
    pub fn git_commit(remote: &str, path: &str, sha: &str) -> Self {
        Locator::Git {
            remote: normalize_remote(remote),
            path: path.to_string(),
            git_ref: GitRef::Commit {
                sha: sha.to_string(),
            },
            host: None,
        }
    }

    /// `Local` locators must not leave the browser without `pack` (§3).
    pub fn is_shareable(&self) -> bool {
        !matches!(self, Locator::Local { .. })
    }

    /// The host adapter for a `Git` locator (explicit, else inferred).
    pub fn git_host(&self) -> Option<GitHost> {
        match self {
            Locator::Git { remote, host, .. } => {
                Some(host.unwrap_or_else(|| GitHost::infer(remote)))
            }
            _ => None,
        }
    }

    /// Structural problems (§6): the entry stays in the file but is
    /// unresolvable; the loader reports these as warnings.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        match self {
            Locator::Git {
                remote,
                path,
                git_ref,
                ..
            } => {
                out.extend(validate_https(remote, "git remote"));
                out.extend(validate_repo_path(path));
                out.extend(git_ref.validate());
            }
            Locator::Relative { path } => {
                if path.is_empty() {
                    out.push("relative path is empty".into());
                } else if path.starts_with('/') {
                    out.push(format!("relative path `{path}` must not start with `/`"));
                }
            }
            Locator::Url { url } => out.extend(validate_https(url, "url")),
            Locator::Local { provider, doc_id } => {
                if provider.is_empty() || doc_id.is_empty() {
                    out.push("local locator needs a provider and a doc_id".into());
                }
            }
            Locator::Embedded => {}
            Locator::Unknown(v) => out.push(format!(
                "unknown locator type `{}` — preserved, unresolvable in this version",
                v.get("type").and_then(Value::as_str).unwrap_or("?")
            )),
        }
        out
    }
}

/// What a source is (§2.3). Unknown kinds are preserved verbatim.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum SourceKind {
    Waffle,
    Step,
    KicadPcb,
    Mesh,
    /// A custom feature script (Rhai text; `Operation::Script` names it by
    /// id — `specs/custom_features_and_modeling_roadmap.md` §A7). Added
    /// 2026-09-19; a new kind, so no reader-floor bump.
    Script,
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum KnownSourceKind {
    Waffle,
    Step,
    KicadPcb,
    Mesh,
    Script,
}

const SOURCE_KIND_TAGS: &[&str] = &["Waffle", "Step", "KicadPcb", "Mesh", "Script"];

impl<'de> Deserialize<'de> for SourceKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(
            match known_or_unknown::<D, KnownSourceKind>(d, SOURCE_KIND_TAGS, "source kind")? {
                Ok(KnownSourceKind::Waffle) => SourceKind::Waffle,
                Ok(KnownSourceKind::Step) => SourceKind::Step,
                Ok(KnownSourceKind::KicadPcb) => SourceKind::KicadPcb,
                Ok(KnownSourceKind::Mesh) => SourceKind::Mesh,
                Ok(KnownSourceKind::Script) => SourceKind::Script,
                Err(v) => SourceKind::Unknown(v),
            },
        )
    }
}

/// The commit actually loaded last time (git locators only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Resolved {
    pub commit: String,
    pub at: DateTime<Utc>,
}

/// Cached content bytes. Encoding tag + decoder shared with the v3 STEP
/// payload (`step_import::STEP_BLOB_ENCODING`, inflation-capped).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Embed {
    pub encoding: String,
    pub blob: String,
}

impl Embed {
    /// Deflate + base64 `text`.
    pub fn from_text(text: &str) -> Self {
        Embed {
            encoding: step_import::STEP_BLOB_ENCODING.to_string(),
            blob: step_import::encode_step_blob(text),
        }
    }

    /// Wrap an already-encoded v3 blob.
    pub fn from_encoded(encoding: &str, blob: &str) -> Self {
        Embed {
            encoding: encoding.to_string(),
            blob: blob.to_string(),
        }
    }

    /// Decode to text. Unknown encodings and over-cap payloads are loud.
    pub fn decode(&self) -> Result<String, String> {
        step_import::decode_step_blob(&self.encoding, &self.blob)
    }
}

/// One row of the `sources` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SourceEntry {
    /// Stable for the life of the document; relinking keeps it.
    pub id: Uuid,
    /// Display name (usually the file's basename).
    pub name: String,
    pub kind: SourceKind,
    pub locator: Locator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolved>,
    /// Algorithm-prefixed hash of the exact bytes last loaded (`crate::hash`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Writer policy: emit `embed`. Absent ⇒ true for `Embedded`, else false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed: Option<Embed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<DateTime<Utc>>,
    /// Unknown keys preserved across load → save (§2.6).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl SourceEntry {
    /// A source whose content came with no origin (file picker, paste):
    /// `Embedded` locator, packed, hashed.
    pub fn embedded(name: impl Into<String>, kind: SourceKind, text: &str) -> Self {
        SourceEntry {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            locator: Locator::Embedded,
            resolved: None,
            content_hash: Some(git_blob_sha1(text.as_bytes())),
            pack: Some(true),
            embed: Some(Embed::from_text(text)),
            fetched_at: None,
            extra: Map::new(),
        }
    }

    /// A linked source with no cached content yet.
    pub fn linked(name: impl Into<String>, kind: SourceKind, locator: Locator) -> Self {
        SourceEntry {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            locator,
            resolved: None,
            content_hash: None,
            pack: None,
            embed: None,
            fetched_at: None,
            extra: Map::new(),
        }
    }

    /// Whether writers should emit `embed` for this entry.
    pub fn effective_pack(&self) -> bool {
        self.pack
            .unwrap_or(matches!(self.locator, Locator::Embedded))
    }

    /// Record freshly loaded content: hash it and cache it iff packed.
    pub fn set_content(&mut self, text: &str) {
        self.content_hash = Some(git_blob_sha1(text.as_bytes()));
        self.embed = self.effective_pack().then(|| Embed::from_text(text));
    }

    /// Loader warnings for this entry (never a load failure).
    pub fn validate(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .locator
            .validate()
            .into_iter()
            .map(|w| format!("source `{}` ({}): {w}", self.name, self.id))
            .collect();
        if let SourceKind::Unknown(v) = &self.kind {
            out.push(format!(
                "source `{}` ({}): unknown source kind `{}` — preserved, unusable in this version",
                self.name,
                self.id,
                v.get("type").and_then(Value::as_str).unwrap_or("?")
            ));
        }
        if let Some(embed) = &self.embed {
            if embed.encoding != step_import::STEP_BLOB_ENCODING {
                out.push(format!(
                    "source `{}` ({}): unknown embed encoding `{}`",
                    self.name, self.id, embed.encoding
                ));
            }
        }
        out
    }
}

/// POSIX-join `dirname(base_path)/rel`, normalizing `.` and `..`. `None` when
/// the result escapes the repository root or is empty.
pub fn join_repo_path(base_path: &str, rel: &str) -> Option<String> {
    let mut out: Vec<&str> = base_path.split('/').collect();
    out.pop(); // the document's own file name
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            s => out.push(s),
        }
    }
    (!out.is_empty()).then(|| out.join("/"))
}

/// Fork-time rebase (`specs/waffle_v4_document_model.md` §7.1): a document
/// opened from a `Git` locator `base` at `commit` is being copied into the
/// user's own storage, where its `Relative` links would no longer resolve
/// (§2.4: they resolve against the document's own location). Every
/// `Relative` entry becomes an absolute `Git` locator in `base`'s repository,
/// **pinned at `commit`** — the content the user saw — with `resolved`
/// recorded. Entry ids are kept, so dependents survive (§2.3). Returns the
/// ids rewritten; a non-`Git` base, or a relative path that escapes the
/// repository, rewrites nothing.
pub fn rebase_relative_sources(
    entries: &mut [SourceEntry],
    base: &Locator,
    commit: &str,
    at: DateTime<Utc>,
) -> Vec<Uuid> {
    let Locator::Git {
        remote,
        path: base_path,
        host,
        ..
    } = base
    else {
        return Vec::new();
    };
    let mut rewritten = Vec::new();
    for entry in entries.iter_mut() {
        let Locator::Relative { path: rel } = &entry.locator else {
            continue;
        };
        let Some(path) = join_repo_path(base_path, rel) else {
            continue;
        };
        entry.locator = Locator::Git {
            remote: normalize_remote(remote),
            path,
            git_ref: GitRef::Commit {
                sha: commit.to_ascii_lowercase(),
            },
            host: *host,
        };
        entry.resolved = Some(Resolved {
            commit: commit.to_ascii_lowercase(),
            at,
        });
        rewritten.push(entry.id);
    }
    rewritten
}
