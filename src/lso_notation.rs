//! Converts DCS LSO grading notation strings into plain-English descriptions.
//!
//! DCS sends grading comments in standard NAVAIR LSO shorthand, e.g.:
//!   `__H__IC (LO)AR FAW __LIG__ WIRE# 3`
//!
//! Each space-separated token encodes an optional modifier, a deviation code,
//! and an optional position code:
//!
//! - Underscores surrounding the deviation code (`_X_`, `__X__`) → "slightly"
//! - Parentheses around the deviation code (`(X)`) → "a little"
//! - No modifier → significant deviation (no adverb)
//!
//! Position codes appear immediately after the deviation+modifiers: `IC`, `AR`, `IM`, `BC`.

/// Deviation code → English phrase (longest codes listed first to avoid partial matches).
const DEVIATIONS: &[(&str, &str)] = &[
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
    ("SLO", "slow"),
    ("LO", "low"),
    ("H", "high"),
    ("F", "fast"),
    ("P", "power"),
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
/// Returns an empty string when the notation contains no recognisable tokens.
///
/// # Example
/// ```
/// # use lso::lso_notation::to_english;
/// assert_eq!(
///     to_english("__H__IC (LO)AR FAW __LIG__ WIRE# 3"),
///     "Slightly high in close, a little low at the ramp, fast all the way, slightly long in the groove"
/// );
/// ```
pub fn to_english(notation: &str) -> String {
    // Strip the wire callout — it is shown separately in the embed/table.
    let base = notation
        .split_once("WIRE#")
        .map(|(left, _)| left)
        .unwrap_or(notation)
        .trim();

    let phrases: Vec<String> = base
        .split_whitespace()
        .filter_map(token_to_phrase)
        .collect();

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

/// Parse a single notation token into a plain-English phrase, or `None` if the token
/// is not a recognised deviation.
fn token_to_phrase(token: &str) -> Option<String> {
    // ── Parenthetical form: "(DEV)POS?" e.g. "(LO)AR" ────────────────────────
    if let Some(rest) = token.strip_prefix('(') {
        let close = rest.find(')')?;
        let dev_code = &rest[..close];
        // Position code follows the closing paren; strip stray underscores.
        let pos_code = rest[close + 1..].trim_matches('_');
        let dev_en = lookup_deviation(dev_code)?;
        let pos_en = lookup_position(pos_code).unwrap_or("");
        return Some(build_phrase("a little ", dev_en, pos_en));
    }

    // ── Underscore form: `_*DEV_*POS?` e.g. "__H__IC", "__LIG__" ─────────────
    // Strip only LEADING underscores; trailing modifiers are handled per-token below.
    let lead_stripped = token.trim_start_matches('_');
    let had_lead_underscores = lead_stripped.len() < token.len();

    // Try to find a known position code as a suffix.
    for &(pos_code, pos_en) in POSITIONS {
        if lead_stripped.ends_with(pos_code) && lead_stripped.len() > pos_code.len() {
            let before_pos = &lead_stripped[..lead_stripped.len() - pos_code.len()];
            // Strip trailing underscores that belong to the modifier.
            let dev_code = before_pos.trim_end_matches('_');
            let had_trail = dev_code.len() < before_pos.len();
            if let Some(dev_en) = lookup_deviation(dev_code) {
                let adverb = if had_lead_underscores || had_trail { "slightly " } else { "" };
                return Some(build_phrase(adverb, dev_en, pos_en));
            }
        }
    }

    // No position code — the whole stripped token is a deviation.
    let dev_code = lead_stripped.trim_end_matches('_');
    let had_trail = dev_code.len() < lead_stripped.len();
    let dev_en = lookup_deviation(dev_code)?;
    let adverb = if had_lead_underscores || had_trail { "slightly " } else { "" };
    Some(build_phrase(adverb, dev_en, ""))
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
    POSITIONS
        .iter()
        .find(|&&(k, _)| k == code)
        .map(|&(_, v)| v)
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
        assert_eq!(to_english("_H_IC UNKNOWN_TOKEN FAW"), "Slightly high in close, fast all the way");
    }
}
