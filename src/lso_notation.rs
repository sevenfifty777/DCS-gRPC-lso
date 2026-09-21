//! LSO shorthand: reads the DCS landing-quality-mark comment into a typed structure, and writes
//! the project's own grading episodes in the same shorthand when DCS wrote nothing.
//!
//! The vocabulary is the NAVAIR 00-80T-104 glossary (`docs/NATOPS/LSO-NATOPS-MAY09.pdf`,
//! sections 11.4.1 to 11.4.3). DCS writes it as:
//!
//! ```text
//! LSO: GRADE:C : _SLOX_  _TMRDAR_  (LURIM)  _DRIM_  WIRE# 2 _EGIW_ [BC]
//! LSO: GRADE:WO  _DRX_  _LURX_  LOIM  _LOIC_  WOFDIC [BC]
//! LSO: GRADE: NC : No proper communications
//! ```
//!
//! - the grade label follows `GRADE:`; `C`, `---`, `OK`, `(OK)`, `_OK_` and `NC` are then
//!   separated from the symbols by ` : `, `WO` and `OWO` by spaces only;
//! - each token is one or more glossary symbols followed by an optional position suffix
//!   (`X` start, `IM` middle, `IC` in close, `AR` ramp, `TL` to land, `IW` in the wires);
//! - parentheses mean "a little", plain text a moderate deviation, and underscores (the NATOPS
//!   underline, "for emphasis") the gross deviation, as `docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md`
//!   reads them; the underline wraps symbol and suffix together (`_LOAR_`);
//! - `WO` inside the body carries the waveoff reason and position (`WO(AFU)IC`, `WOFDIC`,
//!   `WONSUX`); after `WO` the parentheses group the reason, they do not mean "a little";
//! - `WIRE# n` names the wire; `[BC]` marks that the ball call was made, which DCS can only
//!   record when the pilot's communications with the ship are working.
//!
//! Every one of the 160 distinct comments recorded up to 20 September 2026
//! (`tests/fixtures/dcs_lso_comments.txt`) parses with no unknown and no ambiguous token; see
//! `docs/LSO_NOTATION_PARSER_PROTOTYPE_2026-09-21.md`. A token the glossary cannot read is kept
//! verbatim in `Notation::unknown` and rendered as "not understood", never dropped.

use crate::grading::{ApproachZone, EpisodeSeverity, GradingAxis, GradingEpisode};

/// How strongly a deviation is marked: parentheses, plain, underline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Magnitude {
    ALittle,
    Moderate,
    Gross,
}

/// Descriptive symbols, NATOPS 11.4.2. The matcher tries the longest symbols first.
const SYMBOLS: &[(&str, &str)] = &[
    ("AA", "angling approach"),
    ("ACC", "accelerating"),
    ("AFU", "all fouled up"),
    ("B", "flat glideslope"),
    ("C", "climbing"),
    ("CB", "coming back to lineup"),
    ("CD", "coming down"),
    ("CH", "chased"),
    ("CO", "come-on"),
    ("CU", "cocked up"),
    ("DD", "deck down"),
    ("DEC", "decelerating"),
    ("DL", "drifted left"),
    ("DN", "dropped nose"),
    ("DR", "drifted right"),
    ("DU", "deck up"),
    ("EG", "eased gun"),
    ("F", "fast"),
    ("FD", "fouled deck"),
    ("GLI", "gliding approach"),
    ("H", "high"),
    ("HO", "hold off"),
    ("LIG", "long in the groove"),
    ("LL", "landed left"),
    ("LLU", "late lineup"),
    ("LO", "low"),
    ("LR", "landed right"),
    ("LTR", "left to right"),
    ("LU", "lineup"),
    ("LUL", "lined up left"),
    ("LUR", "lined up right"),
    ("LWD", "left wing down"),
    ("N", "nose"),
    ("ND", "nose down"),
    ("NEA", "not enough attitude"),
    ("NEP", "not enough power"),
    ("NERD", "not enough rate of descent"),
    ("NERR", "not enough right rudder"),
    ("NESA", "not enough straightaway"),
    ("NH", "no hook"),
    ("NSU", "not set up"),
    ("OR", "overrotated"),
    ("OS", "overshot"),
    ("OSCB", "overshot coming back"),
    ("P", "power"),
    ("PD", "pitching deck"),
    ("PNU", "pulled nose up"),
    ("ROT", "rotated"),
    ("RUD", "rudder"),
    ("RUF", "rough"),
    ("RWD", "right wing down"),
    ("RR", "right rudder"),
    ("RTL", "right to left"),
    ("S", "settling"),
    ("SD", "spotted the deck"),
    ("SHT", "ship's turn"),
    ("SKD", "skidded"),
    ("SLO", "slow"),
    ("SRD", "stopped rate of descent"),
    ("ST", "steep turn"),
    ("TCA", "too close abeam"),
    ("TMA", "too much attitude"),
    ("TMP", "too much power"),
    ("TMRD", "too much rate of descent"),
    ("TMRR", "too much right rudder"),
    ("TTL", "turned too late"),
    ("TTS", "turned too soon"),
    ("TWA", "too wide abeam"),
    // Glossary: "Wings". Read as "wings not level", which is what the DCS LSO means by `WX`
    // (decision of 21 September 2026).
    ("W", "wings not level"),
    ("WU", "wrapped up"),
    ("XCTL", "cross-controlled"),
    ("LLWD", "landed left wing down"),
    ("LRWD", "landed right wing down"),
    ("LNF", "landed nose first"),
    ("3PTS", "landed three points"),
];

/// Position suffixes, NATOPS 11.4.3.
const SUFFIXES: &[(&str, &str)] = &[
    ("CCA", "on the carrier-controlled approach"),
    ("OT", "out of the turn"),
    ("BC", "at the ball call"),
    ("X", "at the start"),
    ("IM", "in the middle"),
    ("IC", "in close"),
    ("AR", "at the ramp"),
    ("TL", "to land"),
    ("IW", "in the wires"),
    ("AW", "all the way"),
];

/// Grade labels, NATOPS 11.4.1, as DCS writes them after `GRADE:`.
const GRADES: &[(&str, &str)] = &[
    ("_OK_", "perfect pass"),
    ("OK", "OK"),
    ("(OK)", "fair"),
    ("---", "no grade"),
    ("--", "no grade"),
    ("C", "cut"),
    ("WOP", "pattern waveoff"),
    ("OWO", "own waveoff"),
    ("WO", "waveoff"),
    ("B", "bolter"),
    ("NC", "no count"),
];

/// One deviation token: its glossary symbols, how strongly it was marked, and where.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Deviation {
    pub symbols: Vec<&'static str>,
    pub magnitude: Magnitude,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<&'static str>,
    /// The token was a `WO` waveoff call; `symbols` then carry its reason.
    pub waveoff: bool,
}

impl Deviation {
    fn meanings(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.symbols.iter().map(|symbol| symbol_meaning(symbol))
    }

    fn suffix_meaning(&self) -> Option<&'static str> {
        self.suffix.and_then(|suffix| {
            SUFFIXES
                .iter()
                .find(|(code, _)| *code == suffix)
                .map(|(_, meaning)| *meaning)
        })
    }
}

/// A parsed LSO comment, or the project's own episodes written the same way.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Notation {
    /// The `GRADE:` label as DCS wrote it (`C`, `---`, `WO`, ...), when recognised.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grade: Option<&'static str>,
    /// The wire named after `WIRE#`, unfiltered (the caller decides which numbers are wires).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire: Option<u8>,
    pub deviations: Vec<Deviation>,
    pub ball_call: bool,
    /// The free text after a `NC` (no count) label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_text: Option<String>,
    /// Tokens the glossary could not read, verbatim.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unknown: Vec<String>,
}

fn symbol_meaning(symbol: &str) -> &'static str {
    SYMBOLS
        .iter()
        .find(|(code, _)| *code == symbol)
        .map(|(_, meaning)| *meaning)
        .unwrap_or("")
}

/// Symbols sorted longest first, so the matcher never splits `LUL` into `LU` + `L`.
fn symbols_longest_first() -> Vec<&'static str> {
    let mut symbols: Vec<&'static str> = SYMBOLS.iter().map(|(code, _)| *code).collect();
    symbols.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    symbols
}

/// Every way to cut `s` into glossary symbols. Inputs are a few letters long.
fn segmentations(s: &str, symbols: &[&'static str]) -> Vec<Vec<&'static str>> {
    if s.is_empty() {
        return vec![vec![]];
    }
    let mut out = Vec::new();
    for &symbol in symbols {
        if let Some(rest) = s.strip_prefix(symbol) {
            for mut tail in segmentations(rest, symbols) {
                tail.insert(0, symbol);
                out.push(tail);
            }
        }
    }
    out
}

/// Parse a DCS landing-quality-mark comment.
pub fn parse(comment: &str) -> Notation {
    let symbols = symbols_longest_first();
    let mut notation = Notation::default();
    let mut rest = comment.trim();
    if let Some(after) = rest.strip_prefix("LSO:") {
        rest = after.trim_start();
    }
    if let Some(after) = rest.strip_prefix("GRADE:") {
        let after = after.trim_start();
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        let label = &after[..end];
        match GRADES.iter().find(|(code, _)| *code == label) {
            Some((code, _)) => notation.grade = Some(code),
            None if !label.is_empty() => notation.unknown.push(format!("GRADE:{label}")),
            None => {}
        }
        rest = after[end..].trim_start();
        if let Some(after_colon) = rest.strip_prefix(':') {
            rest = after_colon.trim_start();
        }
    }
    if notation.grade == Some("NC") {
        notation.free_text = Some(rest.to_string());
        return notation;
    }

    let mut tokens = rest.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        if let Some(after) = token.strip_prefix("WIRE#") {
            // `WIRE# 3`, `WIRE# 3[BC]`, `WIRE#3`. The number must end the token or be followed
            // by `[`; `1foo` is not a wire.
            let number = if after.is_empty() {
                tokens.next().unwrap_or("")
            } else {
                after
            };
            let digits = number.bytes().take_while(u8::is_ascii_digit).count();
            let tail = &number[digits..];
            match number[..digits].parse::<u8>() {
                Ok(wire) if tail.is_empty() || tail.starts_with('[') => {
                    notation.wire = Some(wire);
                    if !tail.is_empty() {
                        parse_token(tail, &symbols, &mut notation);
                    }
                }
                _ => notation.unknown.push(format!("WIRE# {number}")),
            }
            continue;
        }
        parse_token(token, &symbols, &mut notation);
    }
    notation
}

fn parse_token(token: &str, symbols: &[&'static str], notation: &mut Notation) {
    if token == "[BC]" {
        notation.ball_call = true;
        return;
    }
    let mut core = token;
    let waveoff = core.starts_with("WO");
    if waveoff {
        core = &core[2..];
    }
    let mut magnitude = Magnitude::Moderate;
    let joined;
    if let Some(inner) = core.strip_prefix('(') {
        if let Some(close) = inner.find(')') {
            // `(LURIM)` is "a little"; `WO(AFU)IC` groups the waveoff reason.
            if !waveoff {
                magnitude = Magnitude::ALittle;
            }
            joined = format!("{}{}", &inner[..close], &inner[close + 1..]);
            core = joined.as_str();
        }
    }
    let stripped = core.trim_matches('_');
    if stripped.len() < core.len() {
        magnitude = Magnitude::Gross;
    }
    core = stripped;

    if core.is_empty() {
        if waveoff {
            notation.deviations.push(Deviation {
                symbols: Vec::new(),
                magnitude,
                suffix: None,
                waveoff: true,
            });
        } else if !token.is_empty() {
            notation.unknown.push(token.to_string());
        }
        return;
    }

    // Candidate readings: with each suffix the token ends with, and without a suffix. Prefer a
    // reading with a suffix, then the one with the fewest symbols (longest matches).
    let mut candidates: Vec<(Option<&'static str>, Vec<&'static str>)> = Vec::new();
    for &(suffix, _) in SUFFIXES {
        if let Some(head) = core.strip_suffix(suffix) {
            for reading in segmentations(head, symbols) {
                if !reading.is_empty() || waveoff {
                    candidates.push((Some(suffix), reading));
                }
            }
        }
    }
    for reading in segmentations(core, symbols) {
        candidates.push((None, reading));
    }
    candidates.sort_by(|a, b| {
        b.0.is_some()
            .cmp(&a.0.is_some())
            .then(a.1.len().cmp(&b.1.len()))
    });
    match candidates.into_iter().next() {
        Some((suffix, reading)) => notation.deviations.push(Deviation {
            symbols: reading,
            magnitude,
            suffix,
            waveoff,
        }),
        None => notation.unknown.push(token.to_string()),
    }
}

impl Notation {
    /// The comment's grade label read as a `DcsWaveoffInitiator`-style verdict: `WO` is a
    /// waveoff ordered by the LSO, `OWO` an own waveoff. Everything else, including `WOP`
    /// (pattern waveoff, not seen from DCS yet), names no initiator.
    pub fn waveoff_initiator(&self) -> Option<crate::grading::DcsWaveoffInitiator> {
        match self.grade {
            Some("WO") => Some(crate::grading::DcsWaveoffInitiator::Lso),
            Some("OWO") => Some(crate::grading::DcsWaveoffInitiator::Pilot),
            _ => None,
        }
    }

    /// Plain-English line for Discord and the greenie board. Empty when there is nothing to say.
    pub fn english(&self) -> String {
        if let Some(text) = &self.free_text {
            return capitalise(&match self.grade {
                Some(grade) => format!("{}: {text}", grade_meaning(grade)),
                None => text.clone(),
            });
        }
        let mut phrases: Vec<String> = self.deviations.iter().map(deviation_phrase).collect();
        if let Some(wire) = self.wire {
            phrases.push(format!("wire {wire}"));
        }
        if self.ball_call {
            phrases.push("ball call".to_string());
        }
        if !self.unknown.is_empty() {
            phrases.push(format!("not understood: {}", self.unknown.join(" ")));
        }
        capitalise(&phrases.join(", "))
    }

    /// The shorthand as DCS would write the body: `_LULX_ SLOX (LURIM) WO(AFU)IC`.
    pub fn shorthand(&self) -> String {
        let mut tokens: Vec<String> = self
            .deviations
            .iter()
            .map(|deviation| {
                let body = format!(
                    "{}{}",
                    deviation.symbols.concat(),
                    deviation.suffix.unwrap_or("")
                );
                if deviation.waveoff {
                    let reason = deviation.symbols.concat();
                    let suffix = deviation.suffix.unwrap_or("");
                    if reason.is_empty() {
                        "WO".to_string()
                    } else {
                        format!("WO({reason}){suffix}")
                    }
                } else {
                    match deviation.magnitude {
                        Magnitude::ALittle => format!("({body})"),
                        Magnitude::Moderate => body,
                        Magnitude::Gross => format!("_{body}_"),
                    }
                }
            })
            .collect();
        if let Some(wire) = self.wire {
            tokens.push(format!("WIRE# {wire}"));
        }
        if self.ball_call {
            tokens.push("[BC]".to_string());
        }
        tokens.join(" ")
    }
}

fn grade_meaning(grade: &str) -> &'static str {
    GRADES
        .iter()
        .find(|(code, _)| *code == grade)
        .map(|(_, meaning)| *meaning)
        .unwrap_or("")
}

fn deviation_phrase(deviation: &Deviation) -> String {
    let mut words: Vec<String> = Vec::new();
    if deviation.waveoff {
        words.push("waveoff".to_string());
    }
    if deviation.magnitude == Magnitude::ALittle && !deviation.symbols.is_empty() {
        words.push("a little".to_string());
    }
    // Repeated identical symbols (`PPP`) are repeated calls.
    let meanings: Vec<&'static str> = deviation.meanings().collect();
    let mut index = 0;
    while index < meanings.len() {
        let meaning = meanings[index];
        let mut count = 1;
        while index + count < meanings.len() && meanings[index + count] == meaning {
            count += 1;
        }
        words.push(match count {
            1 => meaning.to_string(),
            2 => format!("{meaning} (two calls)"),
            3 => format!("{meaning} (three calls)"),
            n => format!("{meaning} ({n} calls)"),
        });
        index += count;
    }
    if let Some(suffix) = deviation.suffix_meaning() {
        words.push(suffix.to_string());
    }
    let mut phrase = words.join(" ");
    if deviation.waveoff && deviation.symbols.is_empty() {
        phrase = "waveoff".to_string();
    } else if deviation.waveoff {
        phrase = phrase.replacen("waveoff ", "waveoff: ", 1);
    }
    if deviation.magnitude == Magnitude::Gross {
        phrase.push_str(" (gross)");
    }
    phrase
}

fn capitalise(s: &str) -> String {
    let mut result = s.to_string();
    if let Some(first) = result.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    result
}

/// Convert a DCS LSO notation string to a plain-English sentence. Returns an empty string when
/// the comment contains no recognisable tokens.
pub fn to_english(notation: &str) -> String {
    parse(notation).english()
}

/// The project's own grading episodes written in the same shorthand, for the passes DCS never
/// comments on (no ball call, touch-and-go, straight-in). Only episodes that affected the grade
/// are written, in the order they happened; the size is the episode's peak severity
/// (`Small` a little, `Medium` plain, `Large` underlined) and the position its peak zone.
pub fn from_episodes(episodes: &[GradingEpisode]) -> Notation {
    let deviations = episodes
        .iter()
        .filter(|episode| episode.affects_grade)
        .filter_map(|episode| {
            let magnitude = match episode.maximum_severity {
                EpisodeSeverity::None => return None,
                EpisodeSeverity::Small => Magnitude::ALittle,
                EpisodeSeverity::Medium => Magnitude::Moderate,
                EpisodeSeverity::Large => Magnitude::Gross,
            };
            let symbol = match (episode.axis, episode.peak_classification) {
                (GradingAxis::Glideslope, "high") => "H",
                (GradingAxis::Glideslope, "low") => "LO",
                (GradingAxis::Lineup, "left") => "LUL",
                (GradingAxis::Lineup, "right") => "LUR",
                (GradingAxis::Aoa, "fast" | "slightly_fast") => "F",
                (GradingAxis::Aoa, "slow" | "slightly_slow") => "SLO",
                _ => return None,
            };
            let suffix = match episode.peak_zone {
                ApproachZone::Start => "X",
                ApproachZone::Middle => "IM",
                ApproachZone::InClose => "IC",
                ApproachZone::Ramp => "AR",
            };
            Some(Deviation {
                symbols: vec![symbol],
                magnitude,
                suffix: Some(suffix),
                waveoff: false,
            })
        })
        .collect();
    Notation {
        deviations,
        ..Notation::default()
    }
}

impl Deviation {
    /// The grader's axis this deviation speaks about, when it is one the grader measures.
    /// Glideslope: `H`, `LO`, `B`, `S`, `CD`, `SRD`, `TMRD`, `NERD`. Lineup: `LU`, `LUL`, `LUR`,
    /// `LLU`, `DR`, `DL`, `OS`, `OSCB`, `CB`, `XCTL`. AoA: `F`, `SLO`, `ACC`, `DEC`. A waveoff
    /// call, a power or attitude call, or a landing symbol names no axis.
    pub fn grading_axis(&self) -> Option<GradingAxis> {
        if self.waveoff {
            return None;
        }
        match *self.symbols.first()? {
            "H" | "LO" | "B" | "S" | "CD" | "SRD" | "TMRD" | "NERD" => {
                Some(GradingAxis::Glideslope)
            }
            "LU" | "LUL" | "LUR" | "LLU" | "DR" | "DL" | "OS" | "OSCB" | "CB" | "XCTL" => {
                Some(GradingAxis::Lineup)
            }
            "F" | "SLO" | "ACC" | "DEC" => Some(GradingAxis::Aoa),
            _ => None,
        }
    }

    /// The grader's zone named by the suffix, when it is one of the four groove zones (`TL`,
    /// `IW`, `AW`, `BC` name none).
    pub fn zone(&self) -> Option<ApproachZone> {
        match self.suffix? {
            "X" => Some(ApproachZone::Start),
            "IM" => Some(ApproachZone::Middle),
            "IC" => Some(ApproachZone::InClose),
            "AR" => Some(ApproachZone::Ramp),
            _ => None,
        }
    }
}

/// One axis in one zone: the unit on which the DCS LSO and the grader are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct AxisZone {
    pub axis: GradingAxis,
    pub zone: ApproachZone,
}

impl AxisZone {
    fn order(self) -> (u8, u8) {
        let axis = match self.axis {
            GradingAxis::Glideslope => 0,
            GradingAxis::Lineup => 1,
            GradingAxis::Aoa => 2,
        };
        let zone = match self.zone {
            ApproachZone::Start => 0,
            ApproachZone::Middle => 1,
            ApproachZone::InClose => 2,
            ApproachZone::Ramp => 3,
        };
        (axis, zone)
    }

    fn label(self) -> String {
        let axis = match self.axis {
            GradingAxis::Glideslope => "GS",
            GradingAxis::Lineup => "LU",
            GradingAxis::Aoa => "AoA",
        };
        let zone = match self.zone {
            ApproachZone::Start => "X",
            ApproachZone::Middle => "IM",
            ApproachZone::InClose => "IC",
            ApproachZone::Ramp => "AR",
        };
        format!("{axis}@{zone}")
    }
}

/// Where the DCS LSO and the grader saw a deviation on the same axis in the same zone, where
/// only DCS wrote one, and where only the grader graded one. Sizes are not compared: DCS's
/// modifier and the grader's severity tiers are not calibrated against each other.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Comparison {
    pub agreed: Vec<AxisZone>,
    pub dcs_only: Vec<AxisZone>,
    pub project_only: Vec<AxisZone>,
}

/// Compare a parsed DCS comment with the grader's episodes (only those that affected the grade)
/// axis by axis and zone by zone. DCS symbols outside the three graded axes (power, attitude,
/// landing, waveoff) and positions outside the groove (`TL`, `IW`) take no part.
pub fn compare(dcs: &Notation, episodes: &[GradingEpisode]) -> Comparison {
    fn sorted_unique(mut items: Vec<AxisZone>) -> Vec<AxisZone> {
        items.sort_by_key(|item| item.order());
        items.dedup();
        items
    }
    let dcs_cells = sorted_unique(
        dcs.deviations
            .iter()
            .filter_map(|deviation| {
                Some(AxisZone {
                    axis: deviation.grading_axis()?,
                    zone: deviation.zone()?,
                })
            })
            .collect(),
    );
    let project_cells = sorted_unique(
        episodes
            .iter()
            .filter(|episode| episode.affects_grade)
            .map(|episode| AxisZone {
                axis: episode.axis,
                zone: episode.peak_zone,
            })
            .collect(),
    );
    Comparison {
        agreed: dcs_cells
            .iter()
            .copied()
            .filter(|cell| project_cells.contains(cell))
            .collect(),
        dcs_only: dcs_cells
            .iter()
            .copied()
            .filter(|cell| !project_cells.contains(cell))
            .collect(),
        project_only: project_cells
            .iter()
            .copied()
            .filter(|cell| !dcs_cells.contains(cell))
            .collect(),
    }
}

impl Comparison {
    /// One Markdown cell: `= GS@AR, LU@X · DCS only AoA@X · LSO only GS@IC`, or `agree (3)`
    /// when every cell matches, or `nothing to compare`.
    pub fn summary(&self) -> String {
        let list = |cells: &[AxisZone]| {
            cells
                .iter()
                .map(|cell| cell.label())
                .collect::<Vec<_>>()
                .join(", ")
        };
        if self.agreed.is_empty() && self.dcs_only.is_empty() && self.project_only.is_empty() {
            return "nothing to compare".to_string();
        }
        if self.dcs_only.is_empty() && self.project_only.is_empty() {
            return format!("agree ({})", self.agreed.len());
        }
        let mut parts = Vec::new();
        if !self.agreed.is_empty() {
            parts.push(format!("= {}", list(&self.agreed)));
        }
        if !self.dcs_only.is_empty() {
            parts.push(format!("DCS only {}", list(&self.dcs_only)));
        }
        if !self.project_only.is_empty() {
            parts.push(format!("LSO only {}", list(&self.project_only)));
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grading::{CorrectionQuality, EpisodeEvolution};

    #[test]
    fn dcs_symbols_map_to_the_graded_axes_and_zones() {
        let notation = parse(
            "LSO: GRADE:C : _SLOX_  _TMRDAR_  (LURIM)  _PIC_  WO(AFU)TL  WIRE# 2 _EGIW_ [BC]",
        );
        let cells: Vec<Option<(GradingAxis, ApproachZone)>> = notation
            .deviations
            .iter()
            .map(|d| d.grading_axis().zip(d.zone()))
            .collect();
        assert_eq!(
            cells,
            vec![
                Some((GradingAxis::Aoa, ApproachZone::Start)),
                Some((GradingAxis::Glideslope, ApproachZone::Ramp)),
                Some((GradingAxis::Lineup, ApproachZone::Middle)),
                None, // power in close: not a graded axis
                None, // waveoff
                None, // eased gun in the wires
            ]
        );
    }

    #[test]
    fn comparison_lists_agreement_and_each_side_alone() {
        let dcs = parse("LSO: GRADE:--- : _LULX_  _SLOX_  LOAR  WIRE# 3 _EGIW_ [BC]");
        let episodes = [
            episode(
                GradingAxis::Lineup,
                "left",
                EpisodeSeverity::Large,
                ApproachZone::Start,
                true,
            ),
            episode(
                GradingAxis::Aoa,
                "fast",
                EpisodeSeverity::Medium,
                ApproachZone::Middle,
                true,
            ),
            episode(
                GradingAxis::Glideslope,
                "low",
                EpisodeSeverity::Small,
                ApproachZone::Ramp,
                true,
            ),
            // Diagnostic only: never compared.
            episode(
                GradingAxis::Aoa,
                "slow",
                EpisodeSeverity::Medium,
                ApproachZone::Start,
                false,
            ),
        ];
        let comparison = compare(&dcs, &episodes);
        assert_eq!(
            comparison.summary(),
            "= GS@AR, LU@X · DCS only AoA@X · LSO only AoA@IM"
        );
        assert_eq!(compare(&dcs, &[]).summary(), "DCS only GS@AR, LU@X, AoA@X");
        assert_eq!(
            compare(&parse("LSO: GRADE:_OK_ : WIRE# 3"), &[]).summary(),
            "nothing to compare"
        );
        let same = [
            episode(
                GradingAxis::Lineup,
                "left",
                EpisodeSeverity::Large,
                ApproachZone::Start,
                true,
            ),
            episode(
                GradingAxis::Aoa,
                "slow",
                EpisodeSeverity::Medium,
                ApproachZone::Start,
                true,
            ),
            episode(
                GradingAxis::Glideslope,
                "low",
                EpisodeSeverity::Small,
                ApproachZone::Ramp,
                true,
            ),
        ];
        assert_eq!(compare(&dcs, &same).summary(), "agree (3)");
    }

    /// Every distinct DCS comment recorded up to 20 September 2026.
    const CORPUS: &str = include_str!("../tests/fixtures/dcs_lso_comments.txt");

    fn corpus() -> impl Iterator<Item = &'static str> {
        CORPUS
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    }

    #[test]
    fn every_recorded_comment_parses_without_unknown_tokens() {
        let mut count = 0;
        for comment in corpus() {
            let notation = parse(comment);
            assert!(
                notation.unknown.is_empty(),
                "{comment}: unknown {:?}",
                notation.unknown
            );
            assert!(
                notation.grade.is_some(),
                "{comment}: no grade label recognised"
            );
            count += 1;
        }
        assert_eq!(count, 160);
    }

    #[test]
    fn recorded_grade_labels_are_the_expected_eight() {
        let mut counts = std::collections::BTreeMap::new();
        for comment in corpus() {
            *counts.entry(parse(comment).grade.unwrap()).or_insert(0) += 1;
        }
        assert_eq!(counts["C"], 63);
        assert_eq!(counts["---"], 48);
        assert_eq!(counts["WO"], 36);
        assert_eq!(counts["_OK_"], 4);
        assert_eq!(counts["(OK)"], 4);
        assert_eq!(counts["OK"], 2);
        assert_eq!(counts["OWO"], 2);
        assert_eq!(counts["NC"], 1);
        assert_eq!(counts.len(), 8);
    }

    #[test]
    fn underline_is_gross_and_keeps_the_position() {
        assert_eq!(
            to_english("LSO: GRADE:C : _SLOX_  _TMRDAR_  _DRX_  (LURIM)  _DRIM_  WIRE# 2 _EGIW_ [BC]"),
            "Slow at the start (gross), too much rate of descent at the ramp (gross), \
             drifted right at the start (gross), a little lined up right in the middle, \
             drifted right in the middle (gross), eased gun in the wires (gross), wire 2, ball call"
        );
        assert_eq!(to_english("_LOAR_"), "Low at the ramp (gross)");
        assert_eq!(to_english("LOAR"), "Low at the ramp");
        assert_eq!(to_english("(LOAR)"), "A little low at the ramp");
    }

    #[test]
    fn waveoff_comments_are_not_prefixed_with_spurious_deviations() {
        assert_eq!(
            to_english("LSO: GRADE:WO  WONSUX [BC]"),
            "Waveoff: not set up at the start, ball call"
        );
        assert_eq!(
            to_english("LSO: GRADE:WO  _DRX_  _LURX_  _SLOX_  LOIM  _LOIC_  WOFDIC [BC]"),
            "Drifted right at the start (gross), lined up right at the start (gross), \
             slow at the start (gross), low in the middle, low in close (gross), \
             waveoff: fouled deck in close, ball call"
        );
        assert_eq!(
            to_english("LSO: GRADE:WO  WO(AFU)IC [BC]"),
            "Waveoff: all fouled up in close, ball call"
        );
    }

    #[test]
    fn glossary_symbols_are_read_whole() {
        assert_eq!(to_english("(EGIW)"), "A little eased gun in the wires");
        assert_eq!(to_english("3PTSIW"), "Landed three points in the wires");
        assert_eq!(to_english("(LLIW)"), "A little landed left in the wires");
        assert_eq!(
            to_english("_PPPIC_"),
            "Power (three calls) in close (gross)"
        );
        assert_eq!(to_english("(NX)"), "A little nose at the start");
        assert_eq!(to_english("_LRTL_"), "Landed right to land (gross)");
    }

    #[test]
    fn no_count_keeps_the_free_text() {
        let notation = parse("LSO: GRADE: NC : No proper communications");
        assert_eq!(notation.grade, Some("NC"));
        assert_eq!(
            notation.free_text.as_deref(),
            Some("No proper communications")
        );
        assert_eq!(notation.english(), "No count: No proper communications");
    }

    #[test]
    fn perfect_pass_with_only_a_wire() {
        let notation = parse("LSO: GRADE:_OK_ : WIRE# 3");
        assert_eq!(notation.grade, Some("_OK_"));
        assert_eq!(notation.wire, Some(3));
        assert!(notation.deviations.is_empty());
        assert_eq!(notation.english(), "Wire 3");
    }

    #[test]
    fn wire_number_must_end_the_token_or_be_bracketed() {
        assert_eq!(parse("LSO: GRADE:OK : WIRE# 2[BC]").wire, Some(2));
        assert_eq!(parse("WIRE# 3").wire, Some(3));
        assert_eq!(parse("WIRE#4").wire, Some(4));
        for malformed in [
            "WIRE# 1foo",
            "WIRE# -1",
            "WIRE#",
            "WIRE# 184467440737095516160",
        ] {
            let notation = parse(malformed);
            assert_eq!(notation.wire, None, "{malformed}");
            assert!(!notation.unknown.is_empty(), "{malformed}");
        }
    }

    #[test]
    fn unknown_vocabulary_is_reported_not_dropped() {
        let notation = parse("LSO: GRADE:C : _QQQX_  LOAR  WIRE# 1");
        assert_eq!(notation.unknown, vec!["_QQQX_".to_string()]);
        assert_eq!(
            notation.english(),
            "Low at the ramp, wire 1, not understood: _QQQX_"
        );
    }

    #[test]
    fn waveoff_initiator_comes_from_the_grade_label() {
        use crate::grading::DcsWaveoffInitiator;
        assert_eq!(
            parse("LSO: GRADE:WO  _TMRDAR_  WO(AFU)IC [BC]").waveoff_initiator(),
            Some(DcsWaveoffInitiator::Lso)
        );
        assert_eq!(
            parse("LSO: GRADE:OWO : _LULIM_  WO(AFU)IC [BC]").waveoff_initiator(),
            Some(DcsWaveoffInitiator::Pilot)
        );
        assert_eq!(
            parse("LSO: GRADE:WO : SLOX DRX (LURIM) WO(AFU)IC").waveoff_initiator(),
            Some(DcsWaveoffInitiator::Lso)
        );
        assert_eq!(
            parse("LSO: GRADE:--- : _SLOX_ WIRE# 2").waveoff_initiator(),
            None
        );
        assert_eq!(parse("LSO: GRADE:WOP : NSUX").waveoff_initiator(), None);
    }

    #[test]
    fn shorthand_round_trips_a_recorded_body() {
        let notation =
            parse("LSO: GRADE:C : _SLOX_  (LURIM)  LOAR  WO(AFU)TL  WIRE# 2 _EGIW_ [BC]");
        assert_eq!(
            notation.shorthand(),
            "_SLOX_ (LURIM) LOAR WO(AFU)TL _EGIW_ WIRE# 2 [BC]"
        );
    }

    fn episode(
        axis: GradingAxis,
        classification: &'static str,
        severity: EpisodeSeverity,
        zone: ApproachZone,
        affects_grade: bool,
    ) -> GradingEpisode {
        GradingEpisode {
            axis,
            started_at_dcs: 0.0,
            ended_at_dcs: 1.0,
            duration_s: 1.0,
            most_severe_zone: zone,
            zone_weight: 1.0,
            maximum_severity: severity,
            corrected_severity: severity,
            effective_severity: 1.0,
            peak_value: 0.0,
            peak_normalized_error: 0.0,
            peak_at_dcs: 0.5,
            peak_zone: zone,
            peak_classification: classification,
            evolution: EpisodeEvolution::TowardTarget,
            returned_to_less_severe_band: true,
            oscillation_reversals: 0,
            first_durable_improvement_delay_s: None,
            return_to_none_delay_s: None,
            stabilized_severity: EpisodeSeverity::None,
            stabilization_samples: 2,
            post_correction_aggravation: false,
            correction_reason: "test",
            correction: CorrectionQuality::Good,
            affects_grade,
            diagnostic: None,
        }
    }

    #[test]
    fn measured_episodes_are_written_in_the_same_shorthand() {
        // The 20:52 Justice touch-and-go of 20 September 2026, graded `--` with no DCS comment.
        let episodes = [
            episode(
                GradingAxis::Lineup,
                "left",
                EpisodeSeverity::Large,
                ApproachZone::Start,
                true,
            ),
            episode(
                GradingAxis::Aoa,
                "slow",
                EpisodeSeverity::Medium,
                ApproachZone::Start,
                true,
            ),
            episode(
                GradingAxis::Aoa,
                "fast",
                EpisodeSeverity::Medium,
                ApproachZone::Middle,
                true,
            ),
            episode(
                GradingAxis::Aoa,
                "slightly_slow",
                EpisodeSeverity::Small,
                ApproachZone::Middle,
                true,
            ),
            episode(
                GradingAxis::Aoa,
                "slightly_fast",
                EpisodeSeverity::Small,
                ApproachZone::Ramp,
                false,
            ),
            episode(
                GradingAxis::Aoa,
                "slow",
                EpisodeSeverity::Medium,
                ApproachZone::Ramp,
                true,
            ),
            episode(
                GradingAxis::Glideslope,
                "high",
                EpisodeSeverity::Small,
                ApproachZone::InClose,
                true,
            ),
        ];
        let notation = from_episodes(&episodes);
        assert_eq!(notation.shorthand(), "_LULX_ SLOX FIM (SLOIM) SLOAR (HIC)");
        assert_eq!(
            notation.english(),
            "Lined up left at the start (gross), slow at the start, fast in the middle, \
             a little slow in the middle, slow at the ramp, a little high in close"
        );
        // What the renderer writes, the parser reads back identically.
        assert_eq!(parse(&notation.shorthand()).english(), notation.english());
    }

    #[test]
    fn measured_notation_is_empty_when_nothing_graded() {
        assert_eq!(from_episodes(&[]).english(), "");
        assert_eq!(from_episodes(&[]).shorthand(), "");
    }
}
