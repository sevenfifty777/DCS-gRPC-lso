//! Converts DCS LSO grading notation strings into plain-English descriptions.
//!
//! DCS sends grading comments in standard NAVAIR LSO shorthand, e.g.:
//!   `__H__IC (LO)AR FAW __LIG__ WIRE# 3`
//!
//! Some modules (e.g. VNAO T-45) add a grade prefix:
//!   `LSO: GRADE:--- : (EGIW) WIRE# 4[BC]`
//! That prefix is stripped before parsing.
//!
//! Each space-separated token encodes an optional modifier, one or more deviation
//! codes, and an optional position code:
//!
//! - Underscores surrounding the deviation code (`_X_`, `__X__`) → "slightly"
//! - Parentheses around the deviation code(s) (`(X)`, `(XYZ)`) → "a little" each
//! - No modifier → significant deviation (no adverb)
//!
//! Position codes appear immediately after the deviation+modifiers: `IC`, `AR`, `IM`, `BC`.

/// Deviation code → English phrase.
/// Multi-character codes are listed before single-character ones so the greedy
/// matcher tries them first and avoids splitting e.g. `LO` into `L`+`O`.
const DEVIATIONS: &[(&str, &str)] = &[
    // Multi-character codes
    ("FAW", "fast all the way"),
    ("SAW", "slow all the way"),
    ("LIG", "long in the groove"),
    ("SIG", "short in the groove"),
    ("LULR", "lined up left"),
    ("LURC", "lined up right"),
    ("LULC", "lined up left of centerline"),
    ("NSU", "not set up"),
    ("NWT", "wings not level"),
    ("AFU", "all fouled up"),
    ("EGT", "eased gun"),
    ("LUL", "lined up left"),
    ("LUR", "lined up right"),
    ("SLO", "slow"),
    ("HH", "very high"),
    ("LO", "low"),
    // Single-character codes
    ("H", "high"),
    ("L", "low"),
    ("F", "fast"),
    ("S", "slow"),
    ("P", "power"),
    ("D", "drift"),
    ("T", "turning"),
    // Single-letter codes used by some modules (e.g. VNAO T-45).
    // E = energy/AoA, G = glide slope, I = lineup, W = wings not level.
    ("E", "energy (AoA)"),
    ("G", "glide slope"),
    ("I", "lineup"),
    ("W", "wings not level"),
];

/// Position code → English phrase.
const POSITIONS: &[(&str, &str)] = &[
    ("AR", "at the ramp"),
    ("IM", "in the middle"),
    ("IC", "in close"),
    ("BC", "at ball call"),
];

/// Convert a DCS LSO notation string to a plain-English sentence.
///
/// The WIRE# callout is stripped (the wire number is shown separately).
/// The `LSO: GRADE:xxx :` prefix used by some modules is also stripped.
/// Returns an empty string when the notation contains no recognisable tokens.
pub fn to_english(notation: &str) -> String {
    // Strip "LSO: GRADE:xxx : " prefix (e.g. VNAO T-45 format).
    // The structure is: "LSO: GRADE:--- : <tokens>"
    let notation = notation
        .split_once(" : ")
        .filter(|(prefix, _)| prefix.contains("GRADE"))
        .map(|(_, rest)| rest)
        .unwrap_or(notation);

    // Strip the wire callout — it is shown separately.
    let base = notation
        .split_once("WIRE#")
        .map(|(left, _)| left)
        .unwrap_or(notation)
        .trim();

    // flat_map because one token (e.g. "(EGIW)") can produce multiple phrases.
    let phrases: Vec<String> = base.split_whitespace().flat_map(token_to_phrases).collect();

    if phrases.is_empty() {
        return String::new();
    }

    // Capitalise the first character and join with commas.
    let mut result = phrases.join(", ");
    if let Some(c) = result.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    result
}

/// Parse a single notation token into zero or more plain-English phrases.
///
/// Returns an empty `Vec` when the token contains no recognisable deviations.
/// Returns multiple phrases when a parenthetical contains several codes, e.g. `(EGIW)`.
fn token_to_phrases(token: &str) -> Vec<String> {
    // ── Parenthetical form: "(DEV…)POS?" e.g. "(LO)AR", "(EGIW)" ─────────────
    // The content may be a single code ("LO") or several codes concatenated ("EGIW").
    if let Some(rest) = token.strip_prefix('(') {
        if let Some(close) = rest.find(')') {
            let inner = &rest[..close];
            // Position code follows the closing paren; strip stray underscores.
            let pos_code = rest[close + 1..].trim_matches('_');
            let pos_en = lookup_position(pos_code).unwrap_or("");
            return greedy_decode(inner, "a little ", pos_en);
        }
    }

    // ── Underscore form: `_*DEV_*POS?` e.g. "__H__IC", "__LIG__" ─────────────
    let lead_stripped = token.trim_start_matches('_');
    let had_lead = lead_stripped.len() < token.len();

    // Try to find a known position code as a suffix.
    for &(pos_code, pos_en) in POSITIONS {
        if lead_stripped.ends_with(pos_code) && lead_stripped.len() > pos_code.len() {
            let before_pos = &lead_stripped[..lead_stripped.len() - pos_code.len()];
            let dev_code = before_pos.trim_end_matches('_');
            let had_trail = dev_code.len() < before_pos.len();
            if let Some(dev_en) = lookup_deviation(dev_code) {
                let adverb = if had_lead || had_trail {
                    "slightly "
                } else {
                    ""
                };
                return vec![build_phrase(adverb, dev_en, pos_en)];
            }
        }
    }

    // No position code — strip trailing underscores and match deviation(s).
    let dev_part = lead_stripped.trim_end_matches('_');
    let had_trail = dev_part.len() < lead_stripped.len();
    let adverb = if had_lead || had_trail {
        "slightly "
    } else {
        ""
    };

    // Fast path: entire token is a single known code.
    if let Some(dev_en) = lookup_deviation(dev_part) {
        return vec![build_phrase(adverb, dev_en, "")];
    }

    // Fallback: greedy multi-code decode (handles concatenated codes without parens).
    let decoded = greedy_decode(dev_part, adverb, "");
    if decoded.is_empty() {
        vec![]
    } else {
        decoded
    }
}

/// Greedily consume `s`, matching known deviation codes longest-first.
/// `adverb` is prepended to each phrase; `pos_en` is appended to the final phrase only.
fn greedy_decode(s: &str, adverb: &str, pos_en: &str) -> Vec<String> {
    let mut matches: Vec<(&str, &str)> = Vec::new(); // (adverb, dev_en)
    let mut remaining = s;
    while !remaining.is_empty() {
        let mut found = false;
        for &(code, dev_en) in DEVIATIONS {
            if remaining.starts_with(code) {
                remaining = &remaining[code.len()..];
                matches.push((adverb, dev_en));
                found = true;
                break;
            }
        }
        if !found {
            // Skip one unrecognised character and keep trying.
            let mut chars = remaining.chars();
            chars.next();
            remaining = chars.as_str();
        }
    }
    if matches.is_empty() {
        return vec![];
    }
    let last = matches.len() - 1;
    matches
        .into_iter()
        .enumerate()
        .map(|(i, (adv, dev_en))| {
            if i == last {
                build_phrase(adv, dev_en, pos_en)
            } else {
                build_phrase(adv, dev_en, "")
            }
        })
        .collect()
}

fn build_phrase(adverb: &str, dev: &str, pos: &str) -> String {
    if pos.is_empty() {
        format!("{}{}", adverb, dev)
    } else {
        format!("{}{} {}", adverb, dev, pos)
    }
}

fn lookup_deviation(code: &str) -> Option<&'static str> {
    DEVIATIONS
        .iter()
        .find(|&&(k, _)| k == code)
        .map(|&(_, v)| v)
}

fn lookup_position(code: &str) -> Option<&'static str> {
    if code.is_empty() {
        return None;
    }
    POSITIONS.iter().find(|&&(k, _)| k == code).map(|&(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::to_english;

    #[test]
    fn parses_full_example() {
        assert_eq!(
            to_english("__H__IC (LO)AR FAW __LIG__ WIRE# 3"),
            "Slightly high in close, a little low at the ramp, fast all the way, slightly long in the groove"
        );
    }

    #[test]
    fn parses_standalone_deviation() {
        assert_eq!(to_english("FAW WIRE# 3"), "Fast all the way");
        assert_eq!(to_english("SAW"), "Slow all the way");
    }

    #[test]
    fn parses_underscore_modifier() {
        assert_eq!(to_english("_H_IC"), "Slightly high in close");
        assert_eq!(to_english("_LO_AR"), "Slightly low at the ramp");
    }

    #[test]
    fn parses_plain_deviation_with_position() {
        assert_eq!(to_english("HIC"), "High in close");
        assert_eq!(to_english("LOAR"), "Low at the ramp");
    }

    #[test]
    fn wire_only_returns_empty() {
        assert_eq!(to_english("WIRE# 3"), "");
    }

    #[test]
    fn unknown_token_is_skipped() {
        // XYZQ contains no letter that maps to a known deviation code.
        assert_eq!(
            to_english("_H_IC XYZQ FAW"),
            "Slightly high in close, fast all the way"
        );
    }

    #[test]
    fn strips_vnao_t45_grade_prefix() {
        // Full T-45 format: "LSO: GRADE:--- : (EGIW) WIRE# 4[BC]"
        assert_eq!(
            to_english("LSO: GRADE:--- : (EGIW) WIRE# 4[BC]"),
            "A little energy (AoA), a little glide slope, a little lineup, a little wings not level"
        );
    }

    #[test]
    fn parses_multi_code_parenthetical() {
        assert_eq!(
            to_english("(EGIW)"),
            "A little energy (AoA), a little glide slope, a little lineup, a little wings not level"
        );
    }

    #[test]
    fn strips_prefix_with_ok_grade() {
        assert_eq!(
            to_english("LSO: GRADE:OK : _H_IC WIRE# 2"),
            "Slightly high in close"
        );
    }
}
