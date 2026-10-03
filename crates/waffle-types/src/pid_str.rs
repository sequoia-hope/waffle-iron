//! **The rule: a persistent id crosses every boundary as a decimal STRING.**
//!
//! A persistent entity id ([`crate::kernel::EntityPid`], and what
//! [`crate::Selector::Pid`] stores) is a 64-bit content-seeded hash. A JSON
//! number in JavaScript is an `f64`, so `JSON.parse` silently ROUNDS every
//! id above `2^53` — and a rounded id is not a smaller id, it is a
//! *different entity*.
//!
//! Measured (D4a, 2026-10-03): a plate's edge pid `2216071694111992607`
//! arrived in the page as a different id, and the dimension anchored on it
//! refused as "resolves to no geometry" — the loud failure doing its job
//! about a corruption three layers upstream. Re-measured here, exactly:
//! `node -e "console.log(JSON.parse('2216071694111992607'))"` prints
//! `2216071694111992600`, and the `f64` it holds is
//! `2216071694111992576` — the arithmetic is pinned in
//! `the_same_id_as_a_json_number_would_not_survive_an_f64` below. The quiet
//! half of the same defect is the one that matters: an id that rounds ONTO
//! another live entity resolves, silently, to the wrong face.
//!
//! So there is one rule and one implementation of it. Every `u64` persistent
//! id that is serialized at all — in a `.waffle` file, in an
//! `EngineToUi`/`UiToEngine` message, in an MCP tool argument or result —
//! goes out as a decimal string, through this module:
//!
//! ```ignore
//! #[serde(with = "waffle_types::pid_str")]
//! #[cfg_attr(feature = "json-schema", schemars(with = "String"))]
//! pub pid: u64,
//! ```
//!
//! and `Option<u64>` through [`pid_str::option`](option).
//!
//! **One representation, not two.** Serializing the FILE as numbers and the
//! boundary as strings would need the same type to serialize two ways, which
//! is a per-site decision and therefore a site that can regress — and
//! `Selector::Pid` reaches the page inside a dozen message fields
//! (`ModelUpdated.drawing`, `feature_get`, `names_list`, `assembly_get`,
//! `face_list`, `entity_list`, …), each of which would have to remember. The
//! id is written as a string everywhere instead, which cost one format-floor
//! bump (v8 → v9, `docs/FILE_FORMAT.md` §13) and bought a representation no
//! call site chooses.
//!
//! **Reading accepts both forms**, so every `.waffle` written before the flip
//! still loads and a hand-written tool argument need not quote an id that
//! would have been exact anyway.

use serde::{Deserialize, Deserializer, Serializer};

/// A pid as it arrives: the written form (a decimal string) or a bare JSON
/// number (a pre-v9 file, or a hand-written small id).
#[derive(Deserialize)]
#[serde(untagged)]
enum Either {
    Text(String),
    Number(u64),
}

impl Either {
    fn into_pid<E: serde::de::Error>(self) -> Result<u64, E> {
        match self {
            Either::Text(s) => s.parse().map_err(|_| {
                E::custom(format!(
                    "persistent id {s:?} is not a decimal u64 (ids cross as decimal strings — \
                     waffle_types::pid_str)"
                ))
            }),
            Either::Number(n) => Ok(n),
        }
    }
}

/// Write a pid as a decimal string.
pub fn serialize<S: Serializer>(pid: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&pid.to_string())
}

/// Read a pid from a decimal string or a bare JSON number.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Either::deserialize(d)?.into_pid()
}

/// The same rule for an `Option<u64>` pid — a field a kernel without
/// persistent identity (a mesh-backed import) reports as `null`.
pub mod option {
    use super::Either;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Write `Some(pid)` as a decimal string, `None` as `null`.
    pub fn serialize<S: Serializer>(pid: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        match pid {
            Some(p) => s.serialize_str(&p.to_string()),
            None => s.serialize_none(),
        }
    }

    /// Read an optional pid from a decimal string, a bare JSON number, or
    /// `null`.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        match Option::<Either>::deserialize(d)? {
            Some(e) => e.into_pid().map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "crate::pid_str")]
        pid: u64,
        #[serde(with = "crate::pid_str::option", default)]
        root_pid: Option<u64>,
    }

    /// The id whose rounding was measured, plus one with the TOP bit set —
    /// the worst case for an `f64`.
    const MEASURED: u64 = 2_216_071_694_111_992_607;
    const TOP_BIT: u64 = u64::MAX;

    #[test]
    fn a_pid_is_written_as_a_decimal_string() {
        let json = serde_json::to_value(Holder {
            pid: MEASURED,
            root_pid: Some(TOP_BIT),
        })
        .unwrap();
        assert_eq!(json["pid"], serde_json::json!("2216071694111992607"));
        assert_eq!(json["root_pid"], serde_json::json!("18446744073709551615"));
    }

    #[test]
    fn a_pid_round_trips_exactly_with_the_top_bit_set() {
        let before = Holder {
            pid: TOP_BIT,
            root_pid: Some(MEASURED),
        };
        let text = serde_json::to_string(&before).unwrap();
        assert_eq!(before, serde_json::from_str::<Holder>(&text).unwrap());
    }

    /// Why strings: the same id through a JSON number is a DIFFERENT id.
    /// This is the measurement the module exists for, as arithmetic rather
    /// than as a claim.
    #[test]
    fn the_same_id_as_a_json_number_would_not_survive_an_f64() {
        assert_ne!(MEASURED as f64 as u64, MEASURED);
        assert_eq!(MEASURED as f64 as u64, 2_216_071_694_111_992_576);
        // And the string form is exact for every id, by construction.
        assert_eq!(MEASURED.to_string().parse::<u64>().unwrap(), MEASURED);
    }

    #[test]
    fn a_bare_number_still_reads_so_pre_v9_files_load() {
        let h: Holder = serde_json::from_value(serde_json::json!({
            "pid": 42,
            "root_pid": 7
        }))
        .unwrap();
        assert_eq!(
            h,
            Holder {
                pid: 42,
                root_pid: Some(7)
            }
        );
    }

    #[test]
    fn an_absent_or_null_optional_pid_is_none() {
        let null: Holder =
            serde_json::from_value(serde_json::json!({ "pid": "5", "root_pid": null })).unwrap();
        assert_eq!(null.root_pid, None);
        let absent: Holder = serde_json::from_value(serde_json::json!({ "pid": "5" })).unwrap();
        assert_eq!(absent.root_pid, None);
    }

    #[test]
    fn a_non_numeric_string_is_a_loud_error() {
        let err = serde_json::from_value::<Holder>(serde_json::json!({ "pid": "face-3" }))
            .expect_err("'face-3' is not a u64");
        assert!(
            err.to_string().contains("not a decimal u64"),
            "the error names the rule: {err}"
        );
    }
}
