pub mod assembly_view;
pub mod dispatch;
pub mod engine_state;
pub mod face_refs;
pub mod messages;
pub mod process;
pub mod render_view;
pub mod session;
pub mod stl_export;
pub mod tessellation_runner;
pub mod tools;

#[cfg(target_arch = "wasm32")]
pub mod wasm_api;

pub use dispatch::dispatch;
pub use engine_state::{BridgeError, EngineState};
pub use messages::{EngineToUi, UiToEngine};
pub use session::{DocumentSession, SessionError, TabInfo};
pub use tools::{execute_tool, ExportFile, ToolResult, MAX_AGENT_PAYLOAD_BYTES};
