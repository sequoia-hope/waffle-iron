//! ISO 286-1 limits and fits — the tabulated tolerance grades and fundamental
//! deviations, turned into a pair of limit deviations.
//!
//! Three entry points and nothing else: [`it_grade_um`] for a grade's width,
//! [`hole_deviations`] and [`shaft_deviations`] for a letter/grade class's
//! upper and lower deviation from the nominal. All three take the nominal size
//! in **millimetres** and return **micrometres**, which is the standard's own
//! pair of units — a fit table reads `Ø25 H7 = +21/0 µm` and this module says
//! the same numbers.
//!
//! This file is pure data plus arithmetic. It deliberately depends on nothing
//! else in the crate (no `GeomRef`, no serde, no `thiserror`) because a
//! published table has no reason to know about documents: the consumer that
//! wants a toleranced dimension converts at its own boundary.
//!
//! ## Why it refuses rather than extrapolates
//!
//! Every number here ends up on a manufacturing drawing, and a wrong deviation
//! is not recoverable the way a wrong label is — somebody cuts metal to it. So
//! the module carries only values transcribed from the standard and returns a
//! typed [`Iso286Error`] everywhere else: outside the tabulated size range,
//! outside IT1..=IT18, a letter this table does not carry, or a letter/grade
//! pair the standard does not pair. There is no nearest-step fallback and no
//! extrapolation past Ø500. See the §"Fix It Right or Don't Fix It" posture in
//! `CLAUDE.md`; the same reasoning as
//! [`super::measure::MeasureError`].
//!
//! ## The size steps, and which boundary a step owns
//!
//! The standard's rows read "over 18 up to and including 30", so a nominal
//! that lands exactly on a boundary belongs to the **lower** step: Ø30 is in
//! 18–30, not 30–50. Ø3 is in the first step, which is `0 < D <= 3` (the
//! standard writes it "up to and including 3"; a nominal of exactly 0 has no
//! row and refuses).
//!
//! The first step is also the one where the hole rule's Δ is zero — see
//! §"Δ is zero in the first size step" below.
//!
//! The IT table uses 13 steps (upper bounds 3, 6, 10, 18, 30, 50, 80, 120,
//! 180, 250, 315, 400, 500 mm). The deviation table for `r` and `s` is
//! **finer** than that — it splits 50–80 into 50–65 and 65–80, 80–120 into
//! 80–100 and 100–120, and so on up to 400–450/450–500 — and the finer rows
//! carry different values, so a constant-within-IT-step shortcut would be a
//! silent wrong number for exactly those two letters. Ø100 s6 is `+93/+71`
//! because 100 falls in the 80–100 deviation row (`ei = +71`) against the
//! 80–120 IT row (`IT6 = 22`); a shortcut reading the 80–120 deviation row
//! would be wrong by 8 µm. `d e f g h j k m n p` do not split, and use the
//! IT steps.
//!
//! ## The hole rule
//!
//! Holes are not tabulated independently. ISO 286-1 derives them from the
//! same-letter shaft by the general rule:
//!
//! - `H`: `EI = 0`, `ES = +IT`.
//! - `D E F G` (and `H`, where `es(h) = 0`): `EI = −es(lower-case letter)`,
//!   `ES = EI + IT` — the plain mirror.
//! - `JS`: symmetric about the nominal, like `js`.
//! - `K M N` at grades **≤ IT8**, and `P R S` at grades **≤ IT7**:
//!   `ES = −ei(lower-case letter) + Δ` with `Δ = IT(n) − IT(n−1)`, and
//!   `EI = ES − IT`. The Δ term is what makes `Ø30 K7 = +6/−15` rather than
//!   the mirror's `−2/−23`.
//! - Above those grade limits, `Δ = 0` and the plain mirror applies, which is
//!   why `Ø25 N9 = −15/−67` is exactly the mirror of `Ø25 n9`.
//!
//! `Δ = IT(n) − IT(n−1)` needs an `IT(n−1)`, and there is no IT0 in the
//! standard, so a Δ-bearing hole letter at IT1 is [`Iso286Error::NotTabulated`]
//! rather than a guess — except in the first size step, where Δ is zero at
//! every grade and IT1 therefore needs nothing subtracted.
//!
//! ### Δ is zero in the first size step, `0 < D <= 3`
//!
//! The standard's Δ row for `0 < D <= 3` is **all zeros**, so every Δ-bearing
//! hole letter there is the plain mirror of its shaft. This is not something
//! the `IT(n) − IT(n−1)` arithmetic produces — at Ø2, `IT7 − IT6 = 10 − 6 = 4`
//! — so it is a carve-out this module has to apply, exactly as the `k`
//! carve-out is one.
//!
//! What settles it is that four published hole columns reproduce from the
//! Δ arithmetic across every size step **except** the first, where all four
//! instead equal the mirror:
//!
//! | class | Ø2 published | mirror of the shaft | Δ arithmetic would give |
//! |---|---|---|---|
//! | `K7` | `0/−10`   | `−(+10/0)`   = `0/−10`   | `+4/−6`  |
//! | `K8` | `0/−14`   | `−(+14/0)`   = `0/−14`   | `+4/−10` |
//! | `M7` | `−2/−12`  | `−(+12/+2)`  = `−2/−12`  | `+2/−8`  |
//! | `N7` | `−4/−14`  | `−(+14/+4)`  = `−4/−14`  | `0/−10`  |
//! | `P6` | `−6/−12`  | `−(+12/+6)`  = `−6/−12`  | `−4/−10` |
//! | `P7` | `−6/−16`  | `−(+16/+6)`  = `−6/−16`  | `−2/−12` |
//! | `R7` | `−10/−20` | `−(+20/+10)` = `−10/−20` | `−6/−16` |
//! | `S7` | `−14/−24` | `−(+24/+14)` = `−14/−24` | `−10/−20` |
//!
//! Eight classes, each wrong by exactly its own Δ without the carve-out, and
//! the same four columns (`K7 K8 N7 P6`) land on the published value at every
//! one of the other twelve size steps — so the arithmetic is right and the
//! first row is the exception. Pinned by `PUBLISHED_HOLE` and by
//! `the_first_size_step_has_no_delta_so_its_holes_are_plain_mirrors`.
//!
//! ### `ei(letter)` is the table row, not the shaft's value at that grade
//!
//! The hole rule's `ei(lower-case letter)` is the letter's **tabulated**
//! fundamental deviation. For twelve of the thirteen letters that is the same
//! thing as the shaft's `ei`, but `k` has a grade carve-out — the shaft's `ei`
//! is zero outside IT4..=IT7 — and that carve-out is a shaft-side rule that
//! does not propagate to hole `K`. The two readings disagree at exactly one
//! place, and the published pair settles it: `Ø25 K8 = +10/−23`, which comes
//! out of `ES = −ei(k) + Δ = −2 + 12`. Reading the zeroed shaft value instead
//! would give `+12/−21`.
//!
//! One consequence worth stating because it looks like a bug otherwise:
//! **`k` is the letter whose hole and shaft generally are not mirrors of each
//! other**, so the `d e f g h` mirror check deliberately excludes it. Below
//! IT8 the hole's Δ term breaks the mirror; at IT8 and above the shaft's zero
//! carve-out breaks it instead. It is not a universal law — at a size and
//! grade where Δ happens to equal `ei(k)` the two coincide (Ø4 K3 is one) —
//! so what is pinned is the mechanism rather than the coincidence:
//! `the_hole_rule_reads_the_k_table_row_not_the_shafts_zeroed_value`.
//!
//! ## What this table does NOT carry
//!
//! - **Shaft `a b c cd ef fg t u v x y z za zb zc`** and their hole
//!   counterparts: [`Iso286Error::UnsupportedLetter`]. They are real classes;
//!   this module simply has not transcribed them, and says so by name rather
//!   than inventing them.
//! - **Shaft `j` outside IT5..=IT7**: [`Iso286Error::NotTabulated`]. `j` is one
//!   of the few letters tabulated only for a handful of grades. IT5–IT7 are
//!   transcribed and checked against published pairs; the IT8 row was not
//!   established to this file's confidence bar, so it refuses.
//! - **Hole `J`**: [`Iso286Error::NotTabulated`], and `J` is absent from
//!   [`SUPPORTED_HOLE_LETTERS`] so a UI does not offer it. `J` is the one hole
//!   letter the standard tabulates *directly* rather than deriving — the
//!   mirror of `j` does not reproduce the published `J` column (mirroring
//!   `Ø25 j7 = +13/−8` gives `+8/−13`, where the published `J7` is a different
//!   pair), so deriving it would produce a plausible wrong deviation. That is
//!   the one outcome this module exists to prevent.
//!
//! ## js / JS and an odd IT
//!
//! `js`/`JS` is the symmetric class: `es = +IT/2`, `ei = −IT/2`. Where `ITn` is
//! an **odd whole number of micrometres** ISO 286-1's note keeps both limits
//! whole by using `±(ITn − 1)/2` — **for grades js7 to js11 only**. So
//! `Ø25 js7 = ±10` (not ±10.5) because `IT7 = 21 µm`, while `Ø25 js6 = ±6.5`
//! and `Ø25 js5 = ±4.5`, the half micrometres ISO 286-2 prints in its
//! fine-grade JS columns. [`symmetric`] carries the reasoning for the scope
//! and the one thing that would settle it; `js_symmetric_halves_an_odd_it_by_rounding_down`
//! and `the_odd_it_replacement_is_scoped_to_js7_through_js11` pin both halves.
//! A fractional IT (IT1 at Ø≤3 is 0.8 µm) is halved as-is at every grade — it
//! is already not a whole micrometre, so there is nothing to preserve.
//!
//! Consequence worth stating: `js`/`JS` is the only class whose zone width is
//! not exactly its IT grade. It is `IT − 1` for an odd whole IT **in
//! js7..=js11**, `IT` otherwise, and `zone_width_equals_the_it_grade` asserts
//! that split.

/// Limit deviations from the nominal size, in **micrometres**.
///
/// `upper_um >= lower_um` always holds for a value this module returns, for
/// every letter and grade it accepts —
/// `every_accepted_combination_has_upper_at_or_above_lower` loops the whole
/// table to keep an inverted pair from hiding in one cell.
///
/// These are *deviations*, not sizes: a Ø25 H7 hole is
/// `25 mm + 21 µm / 25 mm + 0 µm`, i.e. 25.021/25.000 mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deviations {
    /// Deviation of the upper limit from the nominal, µm (`ES` for a hole,
    /// `es` for a shaft).
    pub upper_um: f64,
    /// Deviation of the lower limit from the nominal, µm (`EI` / `ei`).
    pub lower_um: f64,
}

/// Why a limit could not be looked up.
///
/// Each arm is a question this table has no tabulated answer to. Not `Eq`:
/// [`Iso286Error::NominalOutOfRange`] carries the offending `f64` so the
/// message can name it.
#[derive(Debug, Clone, PartialEq)]
pub enum Iso286Error {
    /// Nominal outside the tabulated range (this table covers 0 < D <= 500 mm).
    NominalOutOfRange { nominal_mm: f64 },
    /// A tolerance grade outside IT1..=IT18.
    GradeOutOfRange { grade: u8 },
    /// A deviation letter this table does not carry. `supported` lists the ones
    /// it does.
    UnsupportedLetter {
        letter: String,
        supported: &'static str,
    },
    /// The letter/grade pair is not a standard combination (e.g. a hole letter
    /// whose delta rule has no tabulated IT(n-1) because grade is IT1).
    NotTabulated {
        letter: String,
        grade: u8,
        why: &'static str,
    },
}

impl std::fmt::Display for Iso286Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Iso286Error::NominalOutOfRange { nominal_mm } => write!(
                f,
                "nominal size {nominal_mm} mm is outside the ISO 286-1 range this table covers \
                 (0 < D <= 500 mm)"
            ),
            Iso286Error::GradeOutOfRange { grade } => {
                write!(f, "tolerance grade IT{grade} is outside IT1..=IT18",)
            }
            Iso286Error::UnsupportedLetter { letter, supported } => write!(
                f,
                "deviation letter {letter:?} is not carried by this table; it carries {supported}",
            ),
            Iso286Error::NotTabulated { letter, grade, why } => write!(
                f,
                "deviation class {letter}{grade} is not tabulated here: {why}",
            ),
        }
    }
}

impl std::error::Error for Iso286Error {}

/// The shaft deviation letters this table carries, for a diagnostic and for a
/// UI to offer. Lower case, as the standard writes shafts.
pub const SUPPORTED_SHAFT_LETTERS: &[&str] = &[
    "d", "e", "f", "g", "h", "j", "js", "k", "m", "n", "p", "r", "s",
];

/// The hole deviation letters this table carries. Upper case, as the standard
/// writes holes. `J` is deliberately absent — see the module docs.
pub const SUPPORTED_HOLE_LETTERS: &[&str] =
    &["D", "E", "F", "G", "H", "JS", "K", "M", "N", "P", "R", "S"];

/// [`SUPPORTED_SHAFT_LETTERS`] as one string for an error message.
/// `supported_letter_strings_match_the_slices` keeps the two in step.
const SHAFT_LETTERS_STR: &str = "d e f g h j js k m n p r s";

/// [`SUPPORTED_HOLE_LETTERS`] as one string for an error message.
const HOLE_LETTERS_STR: &str = "D E F G H JS K M N P R S";

// ---------------------------------------------------------------------------
// Tables, transcribed from ISO 286-1.
// ---------------------------------------------------------------------------

/// Upper bounds in mm of the 13 IT-table size steps. Each step is "over the
/// previous bound, up to and including this one"; the first is `0 < D <= 3`.
const IT_STEPS_MM: [f64; 13] = [
    3.0, 6.0, 10.0, 18.0, 30.0, 50.0, 80.0, 120.0, 180.0, 250.0, 315.0, 400.0, 500.0,
];

/// IT grade widths in µm: `IT_TABLE_UM[size step][grade − 1]`, IT1..=IT18.
///
/// Grades IT14..=IT18 are, per the standard, not to be used for nominal sizes
/// at or below 1 mm. This table does not enforce that — the first row is one
/// step covering `0 < D <= 3` and has no finer split to hang the rule on.
#[rustfmt::skip]
const IT_TABLE_UM: [[f64; 18]; 13] = [
    // 0 < D <= 3
    [0.8, 1.2, 2.0, 3.0, 4.0, 6.0, 10.0, 14.0, 25.0, 40.0, 60.0, 100.0, 140.0, 250.0, 400.0, 600.0, 1000.0, 1400.0],
    // 3 < D <= 6
    [1.0, 1.5, 2.5, 4.0, 5.0, 8.0, 12.0, 18.0, 30.0, 48.0, 75.0, 120.0, 180.0, 300.0, 480.0, 750.0, 1200.0, 1800.0],
    // 6 < D <= 10
    [1.0, 1.5, 2.5, 4.0, 6.0, 9.0, 15.0, 22.0, 36.0, 58.0, 90.0, 150.0, 220.0, 360.0, 580.0, 900.0, 1500.0, 2200.0],
    // 10 < D <= 18
    [1.2, 2.0, 3.0, 5.0, 8.0, 11.0, 18.0, 27.0, 43.0, 70.0, 110.0, 180.0, 270.0, 430.0, 700.0, 1100.0, 1800.0, 2700.0],
    // 18 < D <= 30
    [1.5, 2.5, 4.0, 6.0, 9.0, 13.0, 21.0, 33.0, 52.0, 84.0, 130.0, 210.0, 330.0, 520.0, 840.0, 1300.0, 2100.0, 3300.0],
    // 30 < D <= 50
    [1.5, 2.5, 4.0, 7.0, 11.0, 16.0, 25.0, 39.0, 62.0, 100.0, 160.0, 250.0, 390.0, 620.0, 1000.0, 1600.0, 2500.0, 3900.0],
    // 50 < D <= 80
    [2.0, 3.0, 5.0, 8.0, 13.0, 19.0, 30.0, 46.0, 74.0, 120.0, 190.0, 300.0, 460.0, 740.0, 1200.0, 1900.0, 3000.0, 4600.0],
    // 80 < D <= 120
    [2.5, 4.0, 6.0, 10.0, 15.0, 22.0, 35.0, 54.0, 87.0, 140.0, 220.0, 350.0, 540.0, 870.0, 1400.0, 2200.0, 3500.0, 5400.0],
    // 120 < D <= 180
    [3.5, 5.0, 8.0, 12.0, 18.0, 25.0, 40.0, 63.0, 100.0, 160.0, 250.0, 400.0, 630.0, 1000.0, 1600.0, 2500.0, 4000.0, 6300.0],
    // 180 < D <= 250
    [4.5, 7.0, 10.0, 14.0, 20.0, 29.0, 46.0, 72.0, 115.0, 185.0, 290.0, 460.0, 720.0, 1150.0, 1850.0, 2900.0, 4600.0, 7200.0],
    // 250 < D <= 315
    [6.0, 8.0, 12.0, 16.0, 23.0, 32.0, 52.0, 81.0, 130.0, 210.0, 320.0, 520.0, 810.0, 1300.0, 2100.0, 3200.0, 5200.0, 8100.0],
    // 315 < D <= 400
    [7.0, 9.0, 13.0, 18.0, 25.0, 36.0, 57.0, 89.0, 140.0, 230.0, 360.0, 570.0, 890.0, 1400.0, 2300.0, 3600.0, 5700.0, 8900.0],
    // 400 < D <= 500
    [8.0, 10.0, 15.0, 20.0, 27.0, 40.0, 63.0, 97.0, 155.0, 250.0, 400.0, 630.0, 970.0, 1550.0, 2500.0, 4000.0, 6300.0, 9700.0],
];

/// Upper bounds in mm of the **fine** deviation size steps, used by `r` and
/// `s`. A superset of [`IT_STEPS_MM`]: the nine extra boundaries (65, 100,
/// 140, 160, 200, 225, 280, 355, 450) split seven IT steps that `r` and `s` do
/// not hold constant across (50–80, 80–120, 120–180, 180–250, 250–315,
/// 315–400 and 400–500).
#[rustfmt::skip]
const FINE_STEPS_MM: [f64; 22] = [
    3.0, 6.0, 10.0, 18.0, 30.0, 50.0, 65.0, 80.0, 100.0, 120.0, 140.0,
    160.0, 180.0, 200.0, 225.0, 250.0, 280.0, 315.0, 355.0, 400.0, 450.0, 500.0,
];

/// Shaft `d`: fundamental deviation `es`, µm, over [`IT_STEPS_MM`].
#[rustfmt::skip]
const D_ES_UM: [f64; 13] = [
    -20.0, -30.0, -40.0, -50.0, -65.0, -80.0, -100.0, -120.0, -145.0, -170.0, -190.0, -210.0, -230.0,
];

/// Shaft `e`: fundamental deviation `es`, µm.
#[rustfmt::skip]
const E_ES_UM: [f64; 13] = [
    -14.0, -20.0, -25.0, -32.0, -40.0, -50.0, -60.0, -72.0, -85.0, -100.0, -110.0, -125.0, -135.0,
];

/// Shaft `f`: fundamental deviation `es`, µm.
#[rustfmt::skip]
const F_ES_UM: [f64; 13] = [
    -6.0, -10.0, -13.0, -16.0, -20.0, -25.0, -30.0, -36.0, -43.0, -50.0, -56.0, -62.0, -68.0,
];

/// Shaft `g`: fundamental deviation `es`, µm.
#[rustfmt::skip]
const G_ES_UM: [f64; 13] = [
    -2.0, -4.0, -5.0, -6.0, -7.0, -9.0, -10.0, -12.0, -14.0, -15.0, -17.0, -18.0, -20.0,
];

/// Shaft `k`: fundamental deviation `ei`, µm, **for grades IT4..=IT7 only**.
/// Outside that band the standard sets `ei = 0`, which [`shaft_ei_um`] applies
/// without consulting this row.
#[rustfmt::skip]
const K_EI_UM: [f64; 13] = [
    0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 4.0, 5.0,
];

/// Shaft `m`: fundamental deviation `ei`, µm.
#[rustfmt::skip]
const M_EI_UM: [f64; 13] = [
    2.0, 4.0, 6.0, 7.0, 8.0, 9.0, 11.0, 13.0, 15.0, 17.0, 20.0, 21.0, 23.0,
];

/// Shaft `n`: fundamental deviation `ei`, µm.
#[rustfmt::skip]
const N_EI_UM: [f64; 13] = [
    4.0, 8.0, 10.0, 12.0, 15.0, 17.0, 20.0, 23.0, 27.0, 31.0, 34.0, 37.0, 40.0,
];

/// Shaft `p`: fundamental deviation `ei`, µm. Constant across each IT step, so
/// it uses [`IT_STEPS_MM`] rather than the fine steps.
#[rustfmt::skip]
const P_EI_UM: [f64; 13] = [
    6.0, 12.0, 15.0, 18.0, 22.0, 26.0, 32.0, 37.0, 43.0, 50.0, 56.0, 62.0, 68.0,
];

/// Shaft `r`: fundamental deviation `ei`, µm, over the **fine** steps
/// [`FINE_STEPS_MM`].
#[rustfmt::skip]
const R_EI_UM: [f64; 22] = [
    10.0, 15.0, 19.0, 23.0, 28.0, 34.0, 41.0, 43.0, 51.0, 54.0, 63.0,
    65.0, 68.0, 77.0, 80.0, 84.0, 94.0, 98.0, 108.0, 114.0, 126.0, 132.0,
];

/// Shaft `s`: fundamental deviation `ei`, µm, over the **fine** steps.
#[rustfmt::skip]
const S_EI_UM: [f64; 22] = [
    14.0, 19.0, 23.0, 28.0, 35.0, 43.0, 53.0, 59.0, 71.0, 79.0, 92.0,
    100.0, 108.0, 122.0, 130.0, 140.0, 158.0, 170.0, 190.0, 208.0, 232.0, 252.0,
];

/// Shaft `j` at IT5 and IT6: fundamental deviation `ei`, µm (negative — `j`
/// straddles the nominal). The standard gives IT5 and IT6 one column.
#[rustfmt::skip]
const J56_EI_UM: [f64; 13] = [
    -2.0, -2.0, -2.0, -3.0, -4.0, -5.0, -7.0, -9.0, -11.0, -13.0, -16.0, -18.0, -20.0,
];

/// Shaft `j` at IT7: fundamental deviation `ei`, µm.
#[rustfmt::skip]
const J7_EI_UM: [f64; 13] = [
    -4.0, -4.0, -5.0, -6.0, -8.0, -10.0, -12.0, -15.0, -18.0, -21.0, -26.0, -28.0, -32.0,
];

/// Why shaft `j` refuses outside IT5..=IT7, and why hole `J` refuses outright.
const J_WHY: &str = "ISO 286-1 tabulates j only for a few grades; this table carries shaft j at \
                     IT5..=IT7 and does not carry hole J at all, because hole J is tabulated \
                     directly rather than mirrored from j and a mirrored value would be wrong";

/// Why a Δ-bearing hole letter refuses at IT1.
const DELTA_AT_IT1_WHY: &str =
    "the hole rule needs Δ = IT(n) − IT(n−1), and ISO 286-1 has no IT0 to subtract";

/// Upper bound in mm of the first size step, which is the one whose Δ row is
/// all zeros. See the module docs §"Δ is zero in the first size step".
const FIRST_STEP_UPPER_MM: f64 = IT_STEPS_MM[0];

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

/// Index of the size step `nominal_mm` falls in, given the steps' upper bounds.
///
/// A boundary belongs to the lower step (the standard's "up to and including"),
/// which is why this is `<=` and not `<`. A NaN or non-positive nominal has no
/// step and refuses rather than landing in step 0 by accident.
fn step_index(nominal_mm: f64, uppers: &[f64]) -> Result<usize, Iso286Error> {
    if nominal_mm.is_nan() || nominal_mm <= 0.0 {
        return Err(Iso286Error::NominalOutOfRange { nominal_mm });
    }
    for (i, &upper) in uppers.iter().enumerate() {
        if nominal_mm <= upper {
            return Ok(i);
        }
    }
    Err(Iso286Error::NominalOutOfRange { nominal_mm })
}

/// The IT grade's tolerance width in µm, e.g. `it_grade_um(25.0, 7) == 21.0`.
///
/// The grade is checked before the nominal, so `it_grade_um(0.0, 7)` names the
/// nominal and `it_grade_um(25.0, 0)` names the grade.
pub fn it_grade_um(nominal_mm: f64, grade: u8) -> Result<f64, Iso286Error> {
    if !(1..=18).contains(&grade) {
        return Err(Iso286Error::GradeOutOfRange { grade });
    }
    let step = step_index(nominal_mm, &IT_STEPS_MM)?;
    Ok(IT_TABLE_UM[step][grade as usize - 1])
}

/// Normalize a caller's letter for matching: trimmed and lower-cased.
///
/// Matching is case-insensitive in both directions. Which table a lookup uses
/// is decided by *which function you call*, not by the case of the letter, so
/// `hole_deviations(25.0, "h", 7)` and `hole_deviations(25.0, "H", 7)` are the
/// same hole and neither is a shaft. That is friendlier than it is strict, and
/// the alternative — rejecting `"h"` as a hole — would refuse something whose
/// meaning is unambiguous.
fn normalize(letter: &str) -> String {
    letter.trim().to_ascii_lowercase()
}

/// Grades over which the odd-IT replacement of [`symmetric`] applies —
/// ISO 286-1's js note is scoped to **js7 to js11**, and this module applies
/// it nowhere else. See [`symmetric`].
const JS_ODD_REPLACEMENT_GRADES: std::ops::RangeInclusive<u8> = 7..=11;

/// The symmetric (`js` / `JS`) zone for an IT width at `grade`.
///
/// The class is `±IT/2` — that is its definition, and it needs no table.
///
/// **The one exception is grade-scoped, and that scope is load-bearing.**
/// ISO 286-1's note on `js` replaces an ODD whole number of micrometres with
/// the even number below it, so that `±(ITn − 1)/2` is a whole micrometre —
/// and it states that for **js7 to js11** only. This module applies it over
/// exactly that range: `Ø25 js7 = ±10` (not ±10.5), because IT7 at 18–30 mm
/// is 21 µm.
///
/// Outside the range the general rule stands, half micrometres and all:
/// `Ø25 js6 = ±6.5` (IT6 = 13 µm) and `Ø25 js5 = ±4.5` (IT5 = 9 µm), which is
/// how ISO 286-2 prints its fine-grade JS columns. Two arguments say the
/// restriction is real rather than editorial. The note's stated PURPOSE is to
/// keep the printed limits whole, which is only a purpose where the table is
/// printed whole. And the range would be pointless otherwise: odd IT values
/// occur at IT5 and IT6 (9 and 13 µm at 18–30 mm), so a lower bound at js7
/// only means anything if those are left alone — while the upper bound at
/// js11 is there because IT12–IT18 have no odd value at any size step, so
/// above it the rule would be vacuous. An unbounded rule would have been
/// written "where ITn is odd", with no range at all.
///
/// **This scope is the one thing in this module resting on recollection of
/// the note's wording rather than on a transcribed table** (found in the M1
/// review, 2026-10-04: the rule had been applied at every grade, and the two
/// `PUBLISHED_SHAFT` rows for Ø25 js5/js6 had been back-filled from the
/// implementation, so the oracle agreed with the code by construction rather
/// than by publication). Applying an exception OUTSIDE its stated range is
/// the extrapolation this module refuses everywhere else, which is why the
/// narrower reading wins on less evidence than the wider one needs. What
/// would settle it beyond doubt is a transcription of ISO 286-2's JS5 and JS6
/// columns; if they print whole micrometres at 18–30 mm, widen
/// [`JS_ODD_REPLACEMENT_GRADES`] and say so here.
///
/// A fractional IT (IT1 at Ø≤3 is 0.8 µm) is halved as-is at every grade — it
/// is already not a whole micrometre, so there is nothing to preserve.
fn symmetric(it_um: f64, grade: u8) -> Deviations {
    let whole_and_odd = it_um.fract() == 0.0 && (it_um as i64) % 2 != 0;
    let half = if whole_and_odd && JS_ODD_REPLACEMENT_GRADES.contains(&grade) {
        (it_um - 1.0) / 2.0
    } else {
        it_um / 2.0
    };
    Deviations {
        upper_um: half,
        lower_um: -half,
    }
}

/// Fundamental deviation `es` (µm, ≤ 0) for shaft letters `d e f g h`.
fn shaft_es_um(nominal_mm: f64, key: &str) -> Result<f64, Iso286Error> {
    if key == "h" {
        // `h` is the zero-deviation shaft by definition, with no table row.
        return Ok(0.0);
    }
    let step = step_index(nominal_mm, &IT_STEPS_MM)?;
    let table = match key {
        "d" => &D_ES_UM,
        "e" => &E_ES_UM,
        "f" => &F_ES_UM,
        "g" => &G_ES_UM,
        _ => unreachable!("shaft_es_um is only called for d e f g h"),
    };
    Ok(table[step])
}

/// The **letter's** tabulated fundamental deviation `ei` (µm, ≥ 0) for
/// `k m n p r s`, read straight off the table row for this size and
/// independent of the grade.
///
/// This is the quantity the hole rule means by `ei(lower-case letter)`. It is
/// deliberately *not* the shaft's `ei` at a given grade — see
/// [`shaft_ei_um`] for the one place those two differ, and why.
fn fundamental_ei_um(nominal_mm: f64, key: &str) -> Result<f64, Iso286Error> {
    match key {
        "k" | "m" | "n" | "p" => {
            let step = step_index(nominal_mm, &IT_STEPS_MM)?;
            let table = match key {
                "k" => &K_EI_UM,
                "m" => &M_EI_UM,
                "n" => &N_EI_UM,
                _ => &P_EI_UM,
            };
            Ok(table[step])
        }
        "r" | "s" => {
            let step = step_index(nominal_mm, &FINE_STEPS_MM)?;
            let table = if key == "r" { &R_EI_UM } else { &S_EI_UM };
            Ok(table[step])
        }
        _ => unreachable!("fundamental_ei_um is only called for k m n p r s"),
    }
}

/// The SHAFT's fundamental deviation `ei` (µm, ≥ 0) for `k m n p r s` at a
/// given grade.
///
/// Identical to [`fundamental_ei_um`] except for `k`, where the standard
/// carves out a grade band: the tabulated `k` deviation applies at IT4..=IT7
/// and the shaft deviation is **zero** at every other grade. So `k8` is
/// `+IT/0` — the zone sits entirely above the nominal, which is the *mirror*
/// of `h8` rather than `h8` itself.
///
/// That carve-out is a shaft-side rule and does **not** propagate to hole `K`:
/// the hole rule reads the letter's tabulated value. `Ø25 K8 = +10/−23` is the
/// published pair that settles it — reading the zeroed shaft value instead
/// would give `+12/−21`. See the module docs.
fn shaft_ei_um(nominal_mm: f64, key: &str, grade: u8) -> Result<f64, Iso286Error> {
    if key == "k" && !(4..=7).contains(&grade) {
        return Ok(0.0);
    }
    fundamental_ei_um(nominal_mm, key)
}

/// Fundamental deviation `ei` (µm, < 0) for shaft `j`, which exists only at
/// IT5..=IT7 here.
fn shaft_j_ei_um(nominal_mm: f64, grade: u8, letter: &str) -> Result<f64, Iso286Error> {
    let table = match grade {
        5 | 6 => &J56_EI_UM,
        7 => &J7_EI_UM,
        _ => {
            return Err(Iso286Error::NotTabulated {
                letter: letter.to_string(),
                grade,
                why: J_WHY,
            })
        }
    };
    let step = step_index(nominal_mm, &IT_STEPS_MM)?;
    Ok(table[step])
}

/// A SHAFT's deviations, e.g. `shaft_deviations(25.0, "g", 6)` →
/// `{upper: -7.0, lower: -20.0}`.
///
/// The letter is matched case-insensitively; `"js"`/`"JS"` both mean the
/// symmetric class. For `d e f g h` the tabulated deviation is the upper one
/// (`es`, ≤ 0) and `ei = es − IT`; for `j k m n p r s` it is the lower one
/// (`ei`) and `es = ei + IT`.
pub fn shaft_deviations(
    nominal_mm: f64,
    letter: &str,
    grade: u8,
) -> Result<Deviations, Iso286Error> {
    let it = it_grade_um(nominal_mm, grade)?;
    let key = normalize(letter);
    match key.as_str() {
        "js" => Ok(symmetric(it, grade)),
        "d" | "e" | "f" | "g" | "h" => {
            let es = shaft_es_um(nominal_mm, &key)?;
            Ok(Deviations {
                upper_um: es,
                lower_um: es - it,
            })
        }
        "j" => {
            let ei = shaft_j_ei_um(nominal_mm, grade, letter)?;
            Ok(Deviations {
                upper_um: ei + it,
                lower_um: ei,
            })
        }
        "k" | "m" | "n" | "p" | "r" | "s" => {
            let ei = shaft_ei_um(nominal_mm, &key, grade)?;
            Ok(Deviations {
                upper_um: ei + it,
                lower_um: ei,
            })
        }
        _ => Err(Iso286Error::UnsupportedLetter {
            letter: letter.to_string(),
            supported: SHAFT_LETTERS_STR,
        }),
    }
}

/// A HOLE's deviations, e.g. `hole_deviations(25.0, "H", 7)` →
/// `{upper: 21.0, lower: 0.0}`.
///
/// The letter is matched case-insensitively against the hole letters (upper
/// case in the standard) and `"JS"`/`"js"` both mean the symmetric class.
/// Holes are derived from the same-letter shaft by the general rule — see the
/// module docs for the rule and its Δ term.
pub fn hole_deviations(
    nominal_mm: f64,
    letter: &str,
    grade: u8,
) -> Result<Deviations, Iso286Error> {
    let it = it_grade_um(nominal_mm, grade)?;
    let key = normalize(letter);
    match key.as_str() {
        "js" => Ok(symmetric(it, grade)),
        // `H` is the mirror of `h` (es = 0), written out because EI = 0 exactly
        // is the definition of the basic hole and not something to compute.
        "h" => Ok(Deviations {
            upper_um: it,
            lower_um: 0.0,
        }),
        "d" | "e" | "f" | "g" => {
            let ei = -shaft_es_um(nominal_mm, &key)?;
            Ok(Deviations {
                upper_um: ei + it,
                lower_um: ei,
            })
        }
        "j" => Err(Iso286Error::NotTabulated {
            letter: letter.to_string(),
            grade,
            why: J_WHY,
        }),
        "k" | "m" | "n" | "p" | "r" | "s" => {
            // The LETTER's tabulated deviation, not the shaft's at this grade:
            // `k`'s zero-outside-IT4..IT7 carve-out is a shaft-side rule.
            let ei_letter = fundamental_ei_um(nominal_mm, &key)?;
            // Δ applies only up to IT8 for K M N and up to IT7 for P R S;
            // above that the plain mirror rule stands. And the standard's Δ row
            // for the first size step is all zeros, so inside `0 < D <= 3` the
            // plain mirror stands at every grade — the `IT(n) − IT(n−1)`
            // arithmetic does NOT yield that (it gives 4 µm at Ø2 IT7), so it
            // has to be carved out. Eight published Ø2 classes settle it; see
            // the module docs.
            let delta_applies = nominal_mm > FIRST_STEP_UPPER_MM
                && match key.as_str() {
                    "k" | "m" | "n" => grade <= 8,
                    _ => grade <= 7,
                };
            let delta = if delta_applies {
                if grade < 2 {
                    return Err(Iso286Error::NotTabulated {
                        letter: letter.to_string(),
                        grade,
                        why: DELTA_AT_IT1_WHY,
                    });
                }
                it - it_grade_um(nominal_mm, grade - 1)?
            } else {
                0.0
            };
            let es = -ei_letter + delta;
            Ok(Deviations {
                upper_um: es,
                lower_um: es - it,
            })
        }
        _ => Err(Iso286Error::UnsupportedLetter {
            letter: letter.to_string(),
            supported: HOLE_LETTERS_STR,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table value is a whole or half micrometre and every derived value
    /// is one addition away from one, so a comparison this tight is still an
    /// exact-value check — it only absorbs the last-bit error of `es − IT`.
    const EPS_UM: f64 = 1e-9;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= EPS_UM
    }

    /// A nominal inside every one of the 22 fine deviation steps, plus every
    /// step boundary. Used by the sweep tests so no size row goes unvisited.
    const PROBE_NOMINALS_MM: &[f64] = &[
        // interior of each fine step
        1.0, 4.0, 8.0, 12.0, 25.0, 40.0, 60.0, 70.0, 90.0, 110.0, 130.0, 150.0, 170.0, 190.0, 210.0,
        240.0, 260.0, 300.0, 330.0, 380.0, 420.0, 480.0,
        // every boundary, which belongs to the step below it
        3.0, 6.0, 10.0, 18.0, 30.0, 50.0, 65.0, 80.0, 100.0, 120.0, 140.0, 160.0, 180.0, 200.0,
        225.0, 250.0, 280.0, 315.0, 355.0, 400.0, 450.0, 500.0,
    ];

    // -----------------------------------------------------------------------
    // The oracle: independently published limit pairs.
    // -----------------------------------------------------------------------

    /// Published ISO 286 shaft limits: `(nominal mm, letter, grade, es µm, ei µm)`.
    ///
    /// Each row is a pair read off a fits table, not a value this
    /// implementation produced. They are spread across letters and size steps
    /// so a single mistranscribed cell shows up: in particular `r` and `s` are
    /// probed inside six different fine steps (Ø60/Ø70 split 50–80, Ø100/Ø110
    /// split 80–120, Ø140/Ø150/Ø170 split 120–180, Ø200 splits 180–250), which
    /// is the check a constant-within-IT-step shortcut fails.
    #[rustfmt::skip]
    const PUBLISHED_SHAFT: &[(f64, &str, u8, f64, f64)] = &[
        // h — the zero shaft, so these pin the IT table directly
        (3.0,   "h",  7,    0.0,  -10.0),
        (6.0,   "h",  7,    0.0,  -12.0),
        (10.0,  "h",  9,    0.0,  -36.0),
        (25.0,  "h",  6,    0.0,  -13.0),
        (25.0,  "h",  7,    0.0,  -21.0),
        (25.0,  "h",  8,    0.0,  -33.0),
        (25.0,  "h", 11,    0.0, -130.0),
        (50.0,  "h", 11,    0.0, -160.0),
        (500.0, "h",  7,    0.0,  -63.0),
        // d e f g — the es-tabulated clearance letters
        (25.0,  "d",  9,  -65.0, -117.0),
        (25.0,  "e",  9,  -40.0,  -92.0),
        (25.0,  "f",  6,  -20.0,  -33.0),
        (25.0,  "f",  7,  -20.0,  -41.0),
        (25.0,  "f",  8,  -20.0,  -53.0),
        (80.0,  "f",  7,  -30.0,  -60.0),
        (25.0,  "g",  6,   -7.0,  -20.0),
        (25.0,  "g",  7,   -7.0,  -28.0),
        // j — straddles the nominal, tabulated only at IT5..IT7
        (10.0,  "j",  6,    7.0,   -2.0),
        (25.0,  "j",  5,    5.0,   -4.0),
        (25.0,  "j",  6,    9.0,   -4.0),
        (25.0,  "j",  7,   13.0,   -8.0),
        (50.0,  "j",  6,   11.0,   -5.0),
        (100.0, "j",  7,   20.0,  -15.0),
        // js — symmetric. Only js7 is an anchor here: it is the grade where
        // ISO 286-1's odd-IT note applies, so ±10 against IT7's 21 µm is a
        // published value that disagrees with the general rule and therefore
        // tests something. js5 and js6 USED to sit here as ±4 and ±6, but
        // those were the module's own output copied into the oracle (M1
        // review, 2026-10-04) — the note is scoped to js7..=js11 and below it
        // the general ±IT/2 stands, giving ±4.5 and ±6.5. They are asserted
        // now by `the_odd_it_replacement_is_scoped_to_js7_through_js11`,
        // which states its provenance as the general rule rather than
        // claiming a published row.
        (25.0,  "js", 7,   10.0,  -10.0),
        // k m n p — the ei-tabulated interference letters, coarse steps
        (25.0,  "k",  6,   15.0,    2.0),
        (50.0,  "k",  6,   18.0,    2.0),
        (25.0,  "m",  6,   21.0,    8.0),
        (25.0,  "n",  6,   28.0,   15.0),
        (25.0,  "p",  6,   35.0,   22.0),
        (50.0,  "p",  6,   42.0,   26.0),
        // r s — the fine-step letters
        (25.0,  "r",  6,   41.0,   28.0),
        (60.0,  "r",  6,   60.0,   41.0),
        (70.0,  "r",  6,   62.0,   43.0),
        (100.0, "r",  6,   73.0,   51.0),
        (140.0, "r",  6,   88.0,   63.0),
        (150.0, "r",  6,   90.0,   65.0),
        (170.0, "r",  6,   93.0,   68.0),
        (25.0,  "s",  6,   48.0,   35.0),
        (60.0,  "s",  7,   83.0,   53.0),
        (100.0, "s",  6,   93.0,   71.0),
        (110.0, "s",  6,  101.0,   79.0),
        (200.0, "s",  6,  151.0,  122.0),
    ];

    /// Published ISO 286 hole limits: `(nominal mm, letter, grade, ES µm, EI µm)`.
    ///
    /// These exercise the derivation rule, not a second table: the `K M N P R
    /// S` rows at IT6–IT8 are the ones that only come out right with the Δ
    /// term, and the IT9 rows are the ones that only come out right without it.
    #[rustfmt::skip]
    const PUBLISHED_HOLE: &[(f64, &str, u8, f64, f64)] = &[
        // H — the basic hole
        (10.0,  "H",  7,   15.0,    0.0),
        (10.0,  "H", 11,   90.0,    0.0),
        (25.0,  "H",  7,   21.0,    0.0),
        (25.0,  "H",  8,   33.0,    0.0),
        (25.0,  "H", 11,  130.0,    0.0),
        (50.0,  "H",  7,   25.0,    0.0),
        (50.0,  "H",  8,   39.0,    0.0),
        (100.0, "H",  7,   35.0,    0.0),
        (100.0, "H",  8,   54.0,    0.0),
        // D E F G — the plain mirror
        (25.0,  "D",  9,  117.0,   65.0),
        (25.0,  "E",  9,   92.0,   40.0),
        (25.0,  "F",  7,   41.0,   20.0),
        (25.0,  "F",  8,   53.0,   20.0),
        (50.0,  "F",  7,   50.0,   25.0),
        (25.0,  "G",  7,   28.0,    7.0),
        (50.0,  "G",  7,   34.0,    9.0),
        // JS — symmetric
        (25.0,  "JS", 7,   10.0,  -10.0),
        // K M N at IT6..IT8 — the Δ term is load-bearing here
        (25.0,  "K",  6,    2.0,  -11.0),
        (25.0,  "K",  8,   10.0,  -23.0),
        (30.0,  "K",  7,    6.0,  -15.0),
        (100.0, "K",  7,   10.0,  -25.0),
        (25.0,  "M",  6,   -4.0,  -17.0),
        (25.0,  "M",  7,    0.0,  -21.0),
        (25.0,  "M",  8,    4.0,  -29.0),
        (25.0,  "N",  6,  -11.0,  -24.0),
        (25.0,  "N",  8,   -3.0,  -36.0),
        (30.0,  "N",  7,   -7.0,  -28.0),
        (100.0, "N",  7,  -10.0,  -45.0),
        // P R S at IT6..IT7 — Δ applies up to IT7 only
        (25.0,  "P",  6,  -18.0,  -31.0),
        (25.0,  "P",  7,  -14.0,  -35.0),
        (100.0, "P",  7,  -24.0,  -59.0),
        (25.0,  "R",  7,  -20.0,  -41.0),
        (100.0, "R",  7,  -38.0,  -73.0),
        (25.0,  "S",  7,  -27.0,  -48.0),
        (100.0, "S",  7,  -58.0,  -93.0),
        // IT9 — above both Δ limits, so the plain mirror
        (25.0,  "N",  9,  -15.0,  -67.0),
        (25.0,  "P",  9,  -22.0,  -74.0),
        // The FIRST size step, where the standard's Δ row is all zeros, so
        // every one of these is the plain mirror even at a grade where Δ would
        // otherwise apply. Without the carve-out each is wrong by its own Δ:
        // K7 would be +4/−6, N7 would be 0/−10, S7 would be −10/−20.
        (2.0,   "K",  7,    0.0,  -10.0),
        (2.0,   "K",  8,    0.0,  -14.0),
        (2.0,   "M",  7,   -2.0,  -12.0),
        (2.0,   "N",  7,   -4.0,  -14.0),
        (2.0,   "P",  6,   -6.0,  -12.0),
        (2.0,   "P",  7,   -6.0,  -16.0),
        (2.0,   "R",  7,  -10.0,  -20.0),
        (2.0,   "S",  7,  -14.0,  -24.0),
        // …and the same at the step's upper boundary, which belongs to it.
        (3.0,   "M",  6,   -2.0,   -8.0),
        (3.0,   "N",  6,   -4.0,  -10.0),
    ];

    #[test]
    fn published_shaft_limits_match() {
        assert!(
            PUBLISHED_SHAFT.len() >= 40,
            "the shaft oracle must stay broad; it has {}",
            PUBLISHED_SHAFT.len()
        );
        for &(nominal, letter, grade, es, ei) in PUBLISHED_SHAFT {
            let got = shaft_deviations(nominal, letter, grade)
                .unwrap_or_else(|e| panic!("Ø{nominal} {letter}{grade} refused: {e}"));
            assert!(
                close(got.upper_um, es) && close(got.lower_um, ei),
                "Ø{nominal} {letter}{grade}: published {es:+}/{ei:+} µm, got \
                 {:+}/{:+} µm",
                got.upper_um,
                got.lower_um
            );
        }
    }

    #[test]
    fn published_hole_limits_match() {
        for &(nominal, letter, grade, upper, lower) in PUBLISHED_HOLE {
            let got = hole_deviations(nominal, letter, grade)
                .unwrap_or_else(|e| panic!("Ø{nominal} {letter}{grade} refused: {e}"));
            assert!(
                close(got.upper_um, upper) && close(got.lower_um, lower),
                "Ø{nominal} {letter}{grade}: published {upper:+}/{lower:+} µm, got \
                 {:+}/{:+} µm",
                got.upper_um,
                got.lower_um
            );
        }
    }

    #[test]
    fn the_oracle_pins_at_least_forty_published_pairs() {
        let total = PUBLISHED_SHAFT.len() + PUBLISHED_HOLE.len();
        assert!(total >= 40, "only {total} published pairs pinned");
    }

    // -----------------------------------------------------------------------
    // Sweeps over every accepting combination.
    // -----------------------------------------------------------------------

    /// Every (function, letter, grade, size step) this table accepts, as the
    /// `Deviations` it returns plus the IT width that should govern it.
    fn accepted_combinations() -> Vec<(&'static str, &'static str, u8, f64, Deviations, f64)> {
        let mut out = Vec::new();
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 1..=18u8 {
                let it = it_grade_um(nominal, grade).expect("probe nominals are all in range");
                for &letter in SUPPORTED_SHAFT_LETTERS {
                    if let Ok(dev) = shaft_deviations(nominal, letter, grade) {
                        out.push(("shaft", letter, grade, nominal, dev, it));
                    }
                }
                for &letter in SUPPORTED_HOLE_LETTERS {
                    if let Ok(dev) = hole_deviations(nominal, letter, grade) {
                        out.push(("hole", letter, grade, nominal, dev, it));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_accepted_combination_has_upper_at_or_above_lower() {
        let all = accepted_combinations();
        assert!(
            all.len() > 5_000,
            "the sweep collapsed to {} rows",
            all.len()
        );
        for (kind, letter, grade, nominal, dev, _) in all {
            assert!(
                dev.upper_um >= dev.lower_um,
                "{kind} Ø{nominal} {letter}{grade} is inverted: {:+}/{:+} µm",
                dev.upper_um,
                dev.lower_um
            );
        }
    }

    #[test]
    fn zone_width_equals_the_it_grade() {
        for (kind, letter, grade, nominal, dev, it) in accepted_combinations() {
            let width = dev.upper_um - dev.lower_um;
            let expected = if letter.eq_ignore_ascii_case("js") {
                // The one documented exception, and it is GRADE-SCOPED: an odd
                // whole IT is halved downwards so both limits stay whole µm,
                // costing 1 µm of zone — for js7..=js11 only (see
                // `symmetric`). Below js7 the general ±IT/2 stands and the
                // zone is exactly IT, half micrometres and all.
                if it.fract() == 0.0
                    && (it as i64) % 2 != 0
                    && JS_ODD_REPLACEMENT_GRADES.contains(&grade)
                {
                    it - 1.0
                } else {
                    it
                }
            } else {
                it
            };
            assert!(
                close(width, expected),
                "{kind} Ø{nominal} {letter}{grade}: zone width {width} µm, expected {expected} µm \
                 (IT{grade} = {it} µm)"
            );
        }
    }

    #[test]
    fn hole_and_shaft_are_mirrored_where_the_mirror_rule_applies() {
        // D E F G H mirror at every grade; M N mirror above IT8 and P R S
        // above IT7, where Δ drops out. `k` is excluded because it never
        // mirrors — see `shaft_k_and_hole_k_are_never_mirrors`.
        let always = ["d", "e", "f", "g", "h"];
        let mn = ["m", "n"];
        let prs = ["p", "r", "s"];
        let mut checked = 0usize;
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 1..=18u8 {
                let mut mirrored: Vec<&str> = always.to_vec();
                if grade > 8 {
                    mirrored.extend_from_slice(&mn);
                }
                if grade > 7 {
                    mirrored.extend_from_slice(&prs);
                }
                for letter in mirrored {
                    let shaft = shaft_deviations(nominal, letter, grade).expect("in range");
                    let hole = hole_deviations(nominal, letter, grade).expect("in range");
                    assert!(
                        close(hole.lower_um, -shaft.upper_um)
                            && close(hole.upper_um, -shaft.lower_um),
                        "Ø{nominal} {letter}{grade} not mirrored: shaft {:+}/{:+}, hole {:+}/{:+}",
                        shaft.upper_um,
                        shaft.lower_um,
                        hole.upper_um,
                        hole.lower_um
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 1_000, "only {checked} mirror pairs checked");
    }

    #[test]
    fn the_first_size_step_has_no_delta_so_its_holes_are_plain_mirrors() {
        // The Δ row for `0 < D <= 3` is all zeros, so EVERY Δ-bearing hole
        // letter there is the exact mirror of its shaft, at every grade —
        // including the grades where Δ otherwise applies, and including IT1,
        // which needs no IT(n−1) once Δ is zero. The `PUBLISHED_HOLE` rows at
        // Ø2/Ø3 are the independent anchors; this is the mechanism across the
        // whole step.
        let mut checked = 0usize;
        for nominal in [0.5, 1.0, 2.0, 3.0] {
            for grade in 1..=18u8 {
                for letter in ["k", "m", "n", "p", "r", "s"] {
                    let hole = hole_deviations(nominal, letter, grade)
                        .unwrap_or_else(|e| panic!("Ø{nominal} {letter}{grade} refused: {e}"));
                    // The mirror is taken against the LETTER's tabulated `ei`,
                    // not the shaft's, so `k`'s grade carve-out does not enter.
                    let ei = fundamental_ei_um(nominal, letter).unwrap();
                    let it = it_grade_um(nominal, grade).unwrap();
                    assert!(
                        close(hole.upper_um, -ei) && close(hole.lower_um, -ei - it),
                        "Ø{nominal} {letter}{grade}: got {:+}/{:+}, the mirror is {:+}/{:+}",
                        hole.upper_um,
                        hole.lower_um,
                        -ei,
                        -ei - it
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked == 4 * 18 * 6, "only {checked} rows checked");

        // And the step BOUNDARY is where it stops: Ø3.0001 is the next step,
        // whose Δ is live, so K7 there is NOT the mirror.
        let past = hole_deviations(3.0001, "K", 7).unwrap();
        let ei = fundamental_ei_um(3.0001, "k").unwrap();
        assert!(
            !close(past.upper_um, -ei),
            "Ø3.0001 K7 = {:+} is still the mirror, so the carve-out leaked past the step",
            past.upper_um
        );
    }

    #[test]
    fn the_delta_term_makes_a_hole_differ_from_the_plain_mirror() {
        // The complement of the mirror test: at the grades where Δ applies, the
        // hole must NOT be the mirror, or the Δ term is not wired in at all.
        for (letter, grade) in [("K", 7u8), ("M", 6), ("N", 7), ("P", 6), ("R", 7), ("S", 7)] {
            let shaft = shaft_deviations(30.0, letter, grade).expect("in range");
            let hole = hole_deviations(30.0, letter, grade).expect("in range");
            assert!(
                !close(hole.upper_um, -shaft.lower_um),
                "Ø30 {letter}{grade} is the plain mirror, so Δ was not applied"
            );
        }
    }

    // -----------------------------------------------------------------------
    // it_grade_um
    // -----------------------------------------------------------------------

    #[test]
    fn it_grade_um_reads_the_published_table() {
        assert_eq!(it_grade_um(25.0, 7).unwrap(), 21.0);
        assert_eq!(it_grade_um(10.0, 7).unwrap(), 15.0);
        assert_eq!(it_grade_um(50.0, 7).unwrap(), 25.0);
        assert_eq!(it_grade_um(100.0, 8).unwrap(), 54.0);
        assert_eq!(it_grade_um(25.0, 6).unwrap(), 13.0);
        assert_eq!(it_grade_um(10.0, 9).unwrap(), 36.0);
        assert_eq!(it_grade_um(1.0, 1).unwrap(), 0.8);
        assert_eq!(it_grade_um(500.0, 18).unwrap(), 9700.0);
    }

    #[test]
    fn it_grade_um_increases_with_grade_and_with_size() {
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 2..=18u8 {
                let smaller = it_grade_um(nominal, grade - 1).unwrap();
                let larger = it_grade_um(nominal, grade).unwrap();
                assert!(
                    larger > smaller,
                    "Ø{nominal}: IT{grade} = {larger} is not above IT{} = {smaller}",
                    grade - 1
                );
            }
        }
        for grade in 1..=18u8 {
            for window in IT_STEPS_MM.windows(2) {
                let (lo, hi) = (window[0], window[1]);
                assert!(
                    it_grade_um(hi, grade).unwrap() >= it_grade_um(lo, grade).unwrap(),
                    "IT{grade} shrinks between Ø{lo} and Ø{hi}"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Size-step boundaries
    // -----------------------------------------------------------------------

    #[test]
    fn a_step_boundary_belongs_to_the_lower_step() {
        // Ø3 is "up to and including 3"; Ø3.0001 is the next row.
        assert_eq!(it_grade_um(3.0, 7).unwrap(), 10.0);
        assert_eq!(it_grade_um(3.0001, 7).unwrap(), 12.0);
        assert_eq!(it_grade_um(30.0, 7).unwrap(), 21.0);
        assert_eq!(it_grade_um(30.0001, 7).unwrap(), 25.0);
    }

    #[test]
    fn a_fine_step_boundary_belongs_to_the_lower_step() {
        // Ø100 s6 = +93/+71 reads the 80–100 deviation row; Ø100.0001 reads
        // 100–120 and is 8 µm further out.
        let at = shaft_deviations(100.0, "s", 6).unwrap();
        assert_eq!((at.upper_um, at.lower_um), (93.0, 71.0));
        let past = shaft_deviations(100.0001, "s", 6).unwrap();
        assert_eq!((past.upper_um, past.lower_um), (101.0, 79.0));
    }

    #[test]
    fn five_hundred_is_in_range_and_anything_past_it_is_not() {
        assert_eq!(it_grade_um(500.0, 7).unwrap(), 63.0);
        assert!(shaft_deviations(500.0, "h", 7).is_ok());
        assert!(hole_deviations(500.0, "H", 7).is_ok());
        assert_eq!(
            it_grade_um(500.0001, 7),
            Err(Iso286Error::NominalOutOfRange {
                nominal_mm: 500.0001
            })
        );
    }

    // -----------------------------------------------------------------------
    // Every error arm is reachable and names the thing.
    // -----------------------------------------------------------------------

    #[test]
    fn a_nominal_of_zero_is_out_of_range() {
        let err = it_grade_um(0.0, 7).unwrap_err();
        assert_eq!(err, Iso286Error::NominalOutOfRange { nominal_mm: 0.0 });
        assert!(err.to_string().contains('0'), "{err}");
        assert_eq!(
            shaft_deviations(0.0, "h", 7).unwrap_err(),
            Iso286Error::NominalOutOfRange { nominal_mm: 0.0 }
        );
        assert_eq!(
            hole_deviations(0.0, "H", 7).unwrap_err(),
            Iso286Error::NominalOutOfRange { nominal_mm: 0.0 }
        );
    }

    #[test]
    fn a_nominal_of_six_hundred_is_out_of_range() {
        for err in [
            it_grade_um(600.0, 7).unwrap_err(),
            shaft_deviations(600.0, "h", 7).unwrap_err(),
            hole_deviations(600.0, "H", 7).unwrap_err(),
        ] {
            assert_eq!(err, Iso286Error::NominalOutOfRange { nominal_mm: 600.0 });
            assert!(err.to_string().contains("600"), "{err}");
        }
    }

    #[test]
    fn a_negative_or_nan_nominal_is_out_of_range() {
        assert_eq!(
            it_grade_um(-25.0, 7).unwrap_err(),
            Iso286Error::NominalOutOfRange { nominal_mm: -25.0 }
        );
        // NaN must refuse rather than fall into step 0 by a failed comparison.
        assert!(matches!(
            it_grade_um(f64::NAN, 7).unwrap_err(),
            Iso286Error::NominalOutOfRange { .. }
        ));
    }

    #[test]
    fn grade_zero_and_grade_nineteen_are_out_of_range() {
        for grade in [0u8, 19, 255] {
            for err in [
                it_grade_um(25.0, grade).unwrap_err(),
                shaft_deviations(25.0, "h", grade).unwrap_err(),
                hole_deviations(25.0, "H", grade).unwrap_err(),
            ] {
                assert_eq!(err, Iso286Error::GradeOutOfRange { grade });
                assert!(err.to_string().contains(&grade.to_string()), "{err}");
            }
        }
    }

    #[test]
    fn an_unsupported_letter_names_itself_and_the_supported_set() {
        // "zc" is a real ISO shaft class this table has not transcribed.
        let err = shaft_deviations(25.0, "zc", 7).unwrap_err();
        assert_eq!(
            err,
            Iso286Error::UnsupportedLetter {
                letter: "zc".to_string(),
                supported: SHAFT_LETTERS_STR,
            }
        );
        assert!(err.to_string().contains("zc"), "{err}");
        assert!(err.to_string().contains("js"), "{err}");

        // "Q" is not a deviation letter at all.
        let err = hole_deviations(25.0, "Q", 7).unwrap_err();
        assert_eq!(
            err,
            Iso286Error::UnsupportedLetter {
                letter: "Q".to_string(),
                supported: HOLE_LETTERS_STR,
            }
        );
        assert!(err.to_string().contains('Q'), "{err}");

        // An empty letter is an unsupported letter, not a panic.
        assert!(matches!(
            shaft_deviations(25.0, "", 7).unwrap_err(),
            Iso286Error::UnsupportedLetter { .. }
        ));
    }

    #[test]
    fn a_letter_grade_pair_the_table_does_not_pair_is_not_tabulated() {
        // Shaft j outside IT5..IT7.
        for grade in [1u8, 4, 8, 9, 18] {
            let err = shaft_deviations(25.0, "j", grade).unwrap_err();
            assert_eq!(
                err,
                Iso286Error::NotTabulated {
                    letter: "j".to_string(),
                    grade,
                    why: J_WHY,
                }
            );
            assert!(err.to_string().contains(&format!("j{grade}")), "{err}");
        }

        // Hole J at any grade.
        for grade in [6u8, 7, 8] {
            assert_eq!(
                hole_deviations(25.0, "J", grade).unwrap_err(),
                Iso286Error::NotTabulated {
                    letter: "J".to_string(),
                    grade,
                    why: J_WHY,
                }
            );
        }

        // A Δ-bearing hole letter at IT1 has no IT0 to subtract.
        for letter in ["K", "M", "N", "P", "R", "S"] {
            let err = hole_deviations(25.0, letter, 1).unwrap_err();
            assert_eq!(
                err,
                Iso286Error::NotTabulated {
                    letter: letter.to_string(),
                    grade: 1,
                    why: DELTA_AT_IT1_WHY,
                }
            );
            assert!(err.to_string().contains("IT0"), "{err}");
        }
        // The same letters at IT2 do have an IT1 to subtract, so they resolve.
        for letter in ["K", "M", "N", "P", "R", "S"] {
            assert!(
                hole_deviations(25.0, letter, 2).is_ok(),
                "{letter}2 refused"
            );
        }
        // The shaft counterparts have no Δ and resolve at IT1.
        for letter in ["k", "m", "n", "p", "r", "s"] {
            assert!(
                shaft_deviations(25.0, letter, 1).is_ok(),
                "{letter}1 refused"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Documented behaviours of individual rules.
    // -----------------------------------------------------------------------

    #[test]
    fn letters_match_case_insensitively() {
        let lower = shaft_deviations(25.0, "g", 6).unwrap();
        assert_eq!(shaft_deviations(25.0, "G", 6).unwrap(), lower);
        assert_eq!(shaft_deviations(25.0, " g ", 6).unwrap(), lower);

        let upper = hole_deviations(25.0, "H", 7).unwrap();
        assert_eq!(hole_deviations(25.0, "h", 7).unwrap(), upper);

        let js = shaft_deviations(25.0, "js", 7).unwrap();
        assert_eq!(shaft_deviations(25.0, "JS", 7).unwrap(), js);
        assert_eq!(hole_deviations(25.0, "js", 7).unwrap(), js);
        assert_eq!(hole_deviations(25.0, "JS", 7).unwrap(), js);
    }

    /// The odd-IT replacement applies over js7..=js11 and NOWHERE ELSE.
    ///
    /// Found in the M1 review (2026-10-04): the rule had been applied at every
    /// grade, so `Ø25 js5` came out ±4 and `js6` ±6, and the two
    /// `PUBLISHED_SHAFT` rows that agreed were the module's own output copied
    /// into the oracle — the one way an anchor table can confirm a defect.
    /// ISO 286-1's note is scoped to js7–js11, and below it the class's
    /// DEFINITION (`±IT/2`) stands with no rounding, which is how ISO 286-2
    /// prints its fine-grade JS columns.
    ///
    /// The provenance of the numbers below is stated rather than implied:
    /// every one is `±IT/2` read off this module's own IT table, so this test
    /// pins the SCOPE of an exception, not a published row. `symmetric`'s doc
    /// comment says what would settle the scope beyond recollection.
    #[test]
    fn the_odd_it_replacement_is_scoped_to_js7_through_js11() {
        // Ø25 is the 18–30 mm step. IT5 = 9 µm and IT6 = 13 µm are both ODD,
        // so this is exactly where an unscoped rule and a scoped one differ.
        for (grade, it) in [(5u8, 9.0), (6, 13.0)] {
            assert_eq!(
                it_grade_um(25.0, grade).unwrap(),
                it,
                "the premise: IT{grade} at Ø25 is an odd whole µm"
            );
            let half = it / 2.0;
            assert_eq!(
                shaft_deviations(25.0, "js", grade).unwrap(),
                Deviations {
                    upper_um: half,
                    lower_um: -half
                },
                "js{grade} is the general ±IT/2 = ±{half}, NOT the js7 note's ±{}",
                (it - 1.0) / 2.0
            );
            // And the hole side, which reads the same function.
            assert_eq!(
                hole_deviations(25.0, "JS", grade).unwrap(),
                shaft_deviations(25.0, "js", grade).unwrap()
            );
        }

        // The boundary from both sides, at a size step where every grade in
        // the range has an odd IT to replace. Inside the range the zone is
        // 1 µm narrow; outside it the zone is exactly IT.
        for grade in 1u8..=18 {
            let it = it_grade_um(25.0, grade).unwrap();
            let dev = shaft_deviations(25.0, "js", grade).unwrap();
            let width = dev.upper_um - dev.lower_um;
            let replaced = it.fract() == 0.0 && (it as i64) % 2 != 0;
            if replaced && (7..=11).contains(&grade) {
                assert!(
                    close(width, it - 1.0),
                    "js{grade} (IT = {it}) is in the note's range, so its zone is IT − 1; got {width}"
                );
            } else {
                assert!(
                    close(width, it),
                    "js{grade} (IT = {it}) is outside the note's range, so its zone is IT; got {width}"
                );
            }
        }

        // The upper bound costs nothing, which is why it is safe: no size step
        // has an odd IT above IT11, so widening or narrowing the range there
        // could not change an answer. Measured rather than asserted.
        for &nominal in &[1.0, 25.0, 100.0, 400.0] {
            for grade in 12u8..=18 {
                let it = it_grade_um(nominal, grade).unwrap();
                assert!(
                    it.fract() != 0.0 || (it as i64) % 2 == 0,
                    "Ø{nominal} IT{grade} = {it} µm is an odd whole µm above IT11, which this \
                     reasoning says does not happen — the js range's upper bound now matters"
                );
            }
        }
    }

    #[test]
    fn js_symmetric_halves_an_odd_it_by_rounding_down() {
        // IT7 at Ø25 is 21 µm — odd, so ±10 and a 20 µm zone, not ±10.5.
        assert_eq!(
            shaft_deviations(25.0, "js", 7).unwrap(),
            Deviations {
                upper_um: 10.0,
                lower_um: -10.0
            }
        );
        // IT8 at Ø25 is 33 µm — also odd: ±16.
        assert_eq!(
            shaft_deviations(25.0, "js", 8).unwrap(),
            Deviations {
                upper_um: 16.0,
                lower_um: -16.0
            }
        );
        // IT9 at Ø25 is 52 µm — even, so exactly ±26.
        assert_eq!(
            shaft_deviations(25.0, "js", 9).unwrap(),
            Deviations {
                upper_um: 26.0,
                lower_um: -26.0
            }
        );
        // IT1 at Ø1 is 0.8 µm — not a whole µm, halved as-is.
        assert_eq!(
            shaft_deviations(1.0, "js", 1).unwrap(),
            Deviations {
                upper_um: 0.4,
                lower_um: -0.4
            }
        );
    }

    #[test]
    fn h_and_capital_h_are_the_basic_shaft_and_hole() {
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 1..=18u8 {
                let it = it_grade_um(nominal, grade).unwrap();
                assert_eq!(
                    shaft_deviations(nominal, "h", grade).unwrap(),
                    Deviations {
                        upper_um: 0.0,
                        lower_um: -it
                    }
                );
                assert_eq!(
                    hole_deviations(nominal, "H", grade).unwrap(),
                    Deviations {
                        upper_um: it,
                        lower_um: 0.0
                    }
                );
            }
        }
    }

    #[test]
    fn shaft_k_sits_entirely_above_the_nominal_outside_it4_to_it7() {
        // The carve-out sets ei = 0, which makes the zone +IT/0 — the mirror
        // of h, not h itself. (h is 0/−IT.)
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 1..=18u8 {
                let it = it_grade_um(nominal, grade).unwrap();
                let k = shaft_deviations(nominal, "k", grade).unwrap();
                if (4..=7).contains(&grade) && nominal > 3.0 {
                    // In band, and above the one size step whose tabulated k
                    // deviation is itself zero, ei is strictly positive.
                    assert!(
                        k.lower_um > 0.0,
                        "Ø{nominal} k{grade} has ei = {:+}, expected the tabulated value",
                        k.lower_um
                    );
                } else {
                    assert_eq!(
                        k,
                        Deviations {
                            upper_um: it,
                            lower_um: 0.0
                        },
                        "Ø{nominal} k{grade} should be +IT/0"
                    );
                }
            }
        }
    }

    #[test]
    fn the_hole_rule_reads_the_k_table_row_not_the_shafts_zeroed_value() {
        // The one place the two readings of `ei(letter)` disagree: grades
        // where `k`'s shaft carve-out applies (outside IT4..=IT7) AND the hole
        // Δ term applies (IT2..=IT8), i.e. IT2, IT3 and IT8. Ø25 K8 = +10/−23
        // in `PUBLISHED_HOLE` is the independent anchor; this asserts the
        // mechanism across every size step, and that the gap between the two
        // readings is exactly the tabulated k deviation.
        for &nominal in PROBE_NOMINALS_MM {
            let step = step_index(nominal, &IT_STEPS_MM).unwrap();
            let ei_table = K_EI_UM[step];
            if ei_table == 0.0 {
                // The first size step tabulates k at zero, so the two
                // readings coincide there and there is nothing to separate.
                continue;
            }
            for grade in [2u8, 3, 8] {
                let it = it_grade_um(nominal, grade).unwrap();
                let delta = it - it_grade_um(nominal, grade - 1).unwrap();
                let hole = hole_deviations(nominal, "K", grade).unwrap();
                assert!(
                    close(hole.upper_um, -ei_table + delta),
                    "Ø{nominal} K{grade}: ES = {:+}, expected −ei(k) + Δ = {:+}",
                    hole.upper_um,
                    -ei_table + delta
                );
                // The zeroed-shaft reading would be `delta`; the gap is ei(k).
                assert!(
                    close(delta - hole.upper_um, ei_table),
                    "Ø{nominal} K{grade}: the two readings differ by {}, not by ei(k) = {ei_table}",
                    delta - hole.upper_um
                );
            }
        }
    }

    #[test]
    fn clearance_letters_sit_at_or_below_zero_and_interference_letters_at_or_above() {
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 1..=18u8 {
                for letter in ["d", "e", "f", "g", "h"] {
                    let dev = shaft_deviations(nominal, letter, grade).unwrap();
                    assert!(
                        dev.upper_um <= 0.0,
                        "Ø{nominal} {letter}{grade} has es = {:+}, above zero",
                        dev.upper_um
                    );
                }
                for letter in ["k", "m", "n", "p", "r", "s"] {
                    let dev = shaft_deviations(nominal, letter, grade).unwrap();
                    assert!(
                        dev.lower_um >= 0.0,
                        "Ø{nominal} {letter}{grade} has ei = {:+}, below zero",
                        dev.lower_um
                    );
                }
            }
        }
        // j is the letter that straddles: ei < 0 < es, at every grade it has.
        for &nominal in PROBE_NOMINALS_MM {
            for grade in 5..=7u8 {
                let dev = shaft_deviations(nominal, "j", grade).unwrap();
                assert!(
                    dev.lower_um < 0.0 && dev.upper_um > 0.0,
                    "Ø{nominal} j{grade} = {:+}/{:+} does not straddle the nominal",
                    dev.upper_um,
                    dev.lower_um
                );
            }
        }
    }

    #[test]
    fn interference_grows_monotonically_through_the_letter_sequence() {
        // d e f g h have ever-smaller clearance; k m n p r s ever-greater
        // interference. A transposed table row would break this ordering.
        for &nominal in PROBE_NOMINALS_MM {
            let es: Vec<f64> = ["d", "e", "f", "g", "h"]
                .iter()
                .map(|l| shaft_deviations(nominal, l, 6).unwrap().upper_um)
                .collect();
            for pair in es.windows(2) {
                assert!(
                    pair[1] > pair[0] || (pair[1] == 0.0 && pair[0] == 0.0),
                    "Ø{nominal}: clearance letters out of order at {pair:?}"
                );
            }
            let ei: Vec<f64> = ["k", "m", "n", "p", "r", "s"]
                .iter()
                .map(|l| shaft_deviations(nominal, l, 6).unwrap().lower_um)
                .collect();
            for pair in ei.windows(2) {
                assert!(
                    pair[1] > pair[0],
                    "Ø{nominal}: interference letters out of order at {pair:?}"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Table self-consistency.
    // -----------------------------------------------------------------------

    #[test]
    fn supported_letter_strings_match_the_slices() {
        assert_eq!(SUPPORTED_SHAFT_LETTERS.join(" "), SHAFT_LETTERS_STR);
        assert_eq!(SUPPORTED_HOLE_LETTERS.join(" "), HOLE_LETTERS_STR);
    }

    #[test]
    fn every_supported_letter_resolves_at_a_grade_it_claims() {
        for &letter in SUPPORTED_SHAFT_LETTERS {
            assert!(
                shaft_deviations(25.0, letter, 6).is_ok(),
                "shaft {letter} is listed as supported but refuses at IT6"
            );
        }
        for &letter in SUPPORTED_HOLE_LETTERS {
            assert!(
                hole_deviations(25.0, letter, 7).is_ok(),
                "hole {letter} is listed as supported but refuses at IT7"
            );
        }
        // J is deliberately NOT listed, and refuses.
        assert!(!SUPPORTED_HOLE_LETTERS.contains(&"J"));
        assert!(hole_deviations(25.0, "J", 7).is_err());
    }

    #[test]
    fn the_fine_steps_are_a_superset_of_the_it_steps() {
        for upper in IT_STEPS_MM {
            assert!(
                FINE_STEPS_MM.contains(&upper),
                "IT step boundary Ø{upper} is missing from the fine steps, so an r/s \
                 lookup could straddle two IT rows"
            );
        }
        for window in FINE_STEPS_MM.windows(2) {
            assert!(window[1] > window[0], "fine steps are not ascending");
        }
        for window in IT_STEPS_MM.windows(2) {
            assert!(window[1] > window[0], "IT steps are not ascending");
        }
    }

    #[test]
    fn r_and_s_actually_differ_inside_an_it_step() {
        // The point of the fine table. If these were equal, a
        // constant-within-IT-step shortcut would be indistinguishable and the
        // Ø100 s6 anchor would be the only thing catching it.
        for letter in ["r", "s"] {
            for (lo, hi) in [(60.0, 70.0), (90.0, 110.0), (130.0, 150.0), (190.0, 210.0)] {
                let a = shaft_deviations(lo, letter, 6).unwrap();
                let b = shaft_deviations(hi, letter, 6).unwrap();
                assert_ne!(
                    a.lower_um, b.lower_um,
                    "{letter} is constant between Ø{lo} and Ø{hi}, inside one IT step"
                );
            }
        }
        // While p, which shares those IT steps, is constant across them.
        for (lo, hi) in [(60.0, 70.0), (90.0, 110.0)] {
            assert_eq!(
                shaft_deviations(lo, "p", 6).unwrap().lower_um,
                shaft_deviations(hi, "p", 6).unwrap().lower_um,
                "p should be constant between Ø{lo} and Ø{hi}"
            );
        }
    }

    #[test]
    fn deviations_is_copy_and_comparable() {
        let a = Deviations {
            upper_um: 21.0,
            lower_um: 0.0,
        };
        let b = a;
        assert_eq!(a, b);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn errors_display_and_are_std_errors() {
        let errs: Vec<Box<dyn std::error::Error>> = vec![
            Box::new(Iso286Error::NominalOutOfRange { nominal_mm: 600.0 }),
            Box::new(Iso286Error::GradeOutOfRange { grade: 19 }),
            Box::new(Iso286Error::UnsupportedLetter {
                letter: "zc".to_string(),
                supported: SHAFT_LETTERS_STR,
            }),
            Box::new(Iso286Error::NotTabulated {
                letter: "J".to_string(),
                grade: 7,
                why: J_WHY,
            }),
        ];
        for err in errs {
            assert!(!err.to_string().is_empty());
        }
    }
}
