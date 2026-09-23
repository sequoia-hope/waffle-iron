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
//! - **The viewport and selection** have no viewer until P-D:
//!   `ViewerUnavailable`, loud.
//! - **The tab and assembly tools** still run in the page's JS (they were
//!   never migrated to Rust): `HostCapability`, loud, until they move.

pub mod documents;
pub mod frames;
pub mod host;

pub use frames::{read_frame, write_frame, Frame, PROTOCOL};
pub use host::Host;
