//! Built-in script library: scripts the engine ships (as text), usable as
//! document sources and pinned by the parity oracles.

/// The involute spur gear as a script (`scripts/gear.rhai`): the built-in
/// generator (`waffle_types::gear::generate_gear_profile`) re-expressed
/// through the sketch API. The A-M2 acceptance gate: its sketch entities
/// and positions equal the generator's exactly (ids, coordinates) —
/// `tests/script_gear_parity.rs`.
pub const GEAR_RHAI: &str = include_str!("../../scripts/gear.rhai");
