//! STEP length-unit detection (roadmap §3.4).
//!
//! truck-stepio ignores unit entities (they usually live inside complex
//! instances, which its typed table drops), so we scan the raw exchange text
//! for the length unit ourselves. OpenCascade/KiCad writes
//! `( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )`; inch files
//! use `CONVERSION_BASED_UNIT('INCH', …)`.

/// Scan STEP text for the file's length unit and return the factor that
/// converts file coordinates to METERS, plus an optional warning when we had
/// to fall back. Default (unit not found): millimeters — the KiCad reality —
/// with a warning.
pub fn scan_length_unit_scale(step_text: &str) -> (f64, Option<String>) {
    // Inch via conversion-based unit (case-insensitive on the unit name).
    let upper = step_text.to_uppercase();
    if upper.contains("CONVERSION_BASED_UNIT") && upper.contains("'INCH'") {
        return (0.0254, None);
    }

    let mut scales: Vec<f64> = Vec::new();
    let mut rest: &str = &upper;
    while let Some(pos) = rest.find("SI_UNIT(") {
        rest = &rest[pos + "SI_UNIT(".len()..];
        let Some(end) = rest.find(')') else { break };
        let args = &rest[..end];
        if !args.contains(".METRE.") {
            continue; // plane-angle radian / steradian units etc.
        }
        let prefix_scale = if args.contains(".MILLI.") {
            1e-3
        } else if args.contains(".CENTI.") {
            1e-2
        } else if args.contains(".DECI.") {
            1e-1
        } else if args.contains(".MICRO.") {
            1e-6
        } else if args.contains(".NANO.") {
            1e-9
        } else if args.contains(".KILO.") {
            1e3
        } else {
            1.0 // bare .METRE.
        };
        scales.push(prefix_scale);
    }

    match scales.as_slice() {
        [] => (
            1e-3,
            Some("no LENGTH_UNIT found in STEP file; assuming millimeters".to_string()),
        ),
        [first, tail @ ..] => {
            let warning = if tail.iter().any(|s| s != first) {
                Some(format!(
                    "multiple distinct length units declared; using the first ({first} m per file unit)"
                ))
            } else {
                None
            };
            (*first, warning)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kicad_millimetre_complex_instance() {
        let text = "#806 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n\
                    #807 = ( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) );";
        let (scale, warn) = scan_length_unit_scale(text);
        assert_eq!(scale, 1e-3);
        assert!(warn.is_none());
    }

    #[test]
    fn bare_metre() {
        let (scale, warn) = scan_length_unit_scale("SI_UNIT($,.METRE.)");
        assert_eq!(scale, 1.0);
        assert!(warn.is_none());
    }

    #[test]
    fn inch_conversion_based() {
        let (scale, _) = scan_length_unit_scale("CONVERSION_BASED_UNIT('INCH',#12) LENGTH_UNIT()");
        assert_eq!(scale, 0.0254);
    }

    #[test]
    fn missing_unit_defaults_to_mm_with_warning() {
        let (scale, warn) = scan_length_unit_scale("DATA; ENDSEC;");
        assert_eq!(scale, 1e-3);
        assert!(warn.is_some());
    }

    #[test]
    fn radian_only_si_unit_is_not_a_length() {
        let (scale, warn) = scan_length_unit_scale("SI_UNIT($,.RADIAN.)");
        assert_eq!(scale, 1e-3);
        assert!(warn.is_some());
    }
}
