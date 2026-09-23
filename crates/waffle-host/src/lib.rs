//! `waffle-host`: the native headless host of the Waffle Iron engine
//! (`specs/waffle_server_mode.md` §2.4, §3.3, S4).
//!
//! The relay spawns one host per session and speaks [`frames`] over its
//! stdio. The host holds ONE document session ([`wasm_bridge::EngineState`])
//! on the real kernel and runs every agent tool through the same
//! [`wasm_bridge::process::process_message`] pipeline the browser worker
//! runs, so a tool answers identically in both hosts (constraint C3). What
//! the page keeps as host state — storage, delivering a download, the
//! viewport, selection — is what [`host::Host`] provides or refuses here:
//!
//! - **Storage** is a file provider rooted at `--documents DIR`: one
//!   `<document id>.waffle` per document, autosaved after every tool that
//!   can change it (§4.8 durability).
//! - **A download** (`export_* deliver:"download"`) is written to
//!   `DIR/exports/` and the answer names the path.
//! - **The viewport and selection** have no viewer attached to the host's
//!   tool router yet: `ViewerUnavailable`, loud. What a viewer DRAWS is
//!   served all the same ([`viewer`]): the document as a snapshot and every
//!   rendered body as a content-addressed blob (§4), which the relay streams
//!   to the browser's `/view` route.
//! - **The tab and assembly tools** moved into the engine on 2026-09-23
//!   (`wasm_bridge::tools::{tabs, assembly}`), so the host holds assemblies.

pub mod documents;
pub mod frames;
pub mod host;
pub mod viewer;

pub use frames::{read_frame, write_frame, Frame, PROTOCOL};
pub use host::Host;
