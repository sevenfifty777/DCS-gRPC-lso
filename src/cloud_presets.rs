//! DCS cloud presets (`Config/Effects/clouds.lua`), reduced to what the recovery-case classifier
//! needs: per preset, the layers (altitudes, coverage) and the precipitation power.
//!
//! The table is generated once from a DCS install by `lso.exe cloud-presets` and checked in as
//! `data/dcs_cloud_presets.json` together with the DCS version it came from; LSO never reads the
//! DCS install at run time. A mission that uses a preset leaves `clouds.density` and
//! `clouds.iprecptns` at 0, so this table is the only source for its coverage and precipitation.
//! How DCS turns a preset into "density out of 10" and "ceiling" is not documented; the
//! classifier's reading of it is a calibration hypothesis (see `crate::recovery_case`).

use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudLayer {
    /// Layer base in the preset's own frame, metres.
    pub altitude_min_m: f64,
    pub altitude_max_m: f64,
    /// Fraction of the sky covered, 0 to 1.
    pub coverage: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPreset {
    #[serde(default)]
    pub readable_name: Option<String>,
    #[serde(default)]
    pub readable_name_short: Option<String>,
    #[serde(default)]
    pub visible_in_gui: Option<bool>,
    /// Above 0 means rain or snow; 0 or -1 means none.
    pub precipitation_power: f64,
    /// Range of the Mission Editor base slider for this preset, metres.
    pub preset_alt_min_m: f64,
    pub preset_alt_max_m: f64,
    /// Layers in file order.
    pub layers: Vec<CloudLayer>,
}

impl CloudPreset {
    /// Layers sorted by base altitude.
    pub fn layers_by_altitude(&self) -> Vec<&CloudLayer> {
        let mut layers: Vec<&CloudLayer> = self.layers.iter().collect();
        layers.sort_by(|a, b| a.altitude_min_m.total_cmp(&b.altitude_min_m));
        layers
    }

    /// Lowest layer whose coverage is strictly above `min_coverage` (or at least it, with
    /// `inclusive`).
    pub fn lowest_layer_with_coverage(
        &self,
        min_coverage: f64,
        inclusive: bool,
    ) -> Option<&CloudLayer> {
        self.layers_by_altitude().into_iter().find(|layer| {
            if inclusive {
                layer.coverage >= min_coverage
            } else {
                layer.coverage > min_coverage
            }
        })
    }

    pub fn has_precipitation(&self) -> bool {
        self.precipitation_power > 0.0
    }

    /// Cloud-cover codes of ED's own METAR description in `readable_name` (e.g.
    /// `"METAR: BKN/OVC LYR 7/13 20/22"`), lowest layer first, each turned into a density out
    /// of 10: FEW 2, SCT 4, BKN 7, OVC 9, and the mean for a pair such as `BKN/OVC`. Weather
    /// and other tokens (`RA`, `TS`, `VIS 3-5KM`, `LYR`, heights) are skipped. Empty when the
    /// preset has no METAR text. `coverage` is a rendering parameter (even the "Overcast"
    /// presets stay below 0.9), so this text is the better reading of how cloudy ED means a
    /// preset to be; whether ED's ATC uses the same value is a calibration question.
    pub fn metar_cover_densities(&self) -> Vec<f64> {
        let Some(name) = &self.readable_name else {
            return Vec::new();
        };
        let Some((_, metar)) = name.split_once("METAR:") else {
            return Vec::new();
        };
        metar
            .split_whitespace()
            .filter_map(|token| {
                let mut total = 0.0;
                let mut count = 0.0;
                for part in token.split('/') {
                    total += match part.to_ascii_uppercase().as_str() {
                        "FEW" => 2.0,
                        "SCT" => 4.0,
                        "BKN" => 7.0,
                        "OVC" => 9.0,
                        _ => return None,
                    };
                    count += 1.0;
                }
                Some(total / count)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPresetTable {
    /// Path of the source file inside the DCS install.
    pub source: String,
    /// DCS version of the install the table was generated from (`autoupdate.cfg`).
    pub dcs_version: Option<String>,
    pub presets: BTreeMap<String, CloudPreset>,
}

impl CloudPresetTable {
    pub fn get(&self, name: &str) -> Option<&CloudPreset> {
        self.presets.get(name)
    }
}

/// The checked-in table. `data/dcs_cloud_presets.json` is embedded at compile time and a unit
/// test parses it, so this cannot fail on a built binary.
pub static CLOUD_PRESETS: Lazy<CloudPresetTable> = Lazy::new(|| {
    serde_json::from_str(include_str!("../data/dcs_cloud_presets.json"))
        .expect("data/dcs_cloud_presets.json is covered by a unit test")
});

/// Error while reading `clouds.lua`.
#[derive(Debug, thiserror::Error)]
#[error("clouds.lua, byte {offset}: {message}")]
pub struct CloudsLuaError {
    pub offset: usize,
    pub message: String,
}

/// Parse the text of DCS `Config/Effects/clouds.lua` into a preset table.
pub fn parse_clouds_lua(
    text: &str,
    dcs_version: Option<String>,
) -> Result<CloudPresetTable, CloudsLuaError> {
    let statements = LuaParser::new(text)?.parse_chunk()?;
    let clouds = statements
        .iter()
        .find(|(name, _)| name == "clouds")
        .map(|(_, value)| value)
        .ok_or_else(|| err(0, "no top-level `clouds` table"))?;
    let presets_value = clouds
        .field("presets")
        .ok_or_else(|| err(0, "`clouds.presets` not found"))?;
    let LuaValue::Table(entries) = presets_value else {
        return Err(err(0, "`clouds.presets` is not a table"));
    };

    let mut presets = BTreeMap::new();
    for (key, value) in entries {
        let Some(LuaKey::Name(name)) = key else {
            continue;
        };
        let preset = preset_from_lua(name, value)?;
        presets.insert(name.clone(), preset);
    }
    if presets.is_empty() {
        return Err(err(0, "`clouds.presets` holds no preset"));
    }
    Ok(CloudPresetTable {
        source: "Config/Effects/clouds.lua".to_string(),
        dcs_version,
        presets,
    })
}

fn preset_from_lua(name: &str, value: &LuaValue) -> Result<CloudPreset, CloudsLuaError> {
    let number = |key: &str| -> Result<f64, CloudsLuaError> {
        value
            .field(key)
            .and_then(LuaValue::as_number)
            .ok_or_else(|| err(0, format!("preset `{name}`: numeric `{key}` missing")))
    };
    let Some(LuaValue::Table(layer_entries)) = value.field("layers") else {
        return Err(err(0, format!("preset `{name}`: `layers` table missing")));
    };
    let mut layers = Vec::new();
    for (index, (_, layer)) in layer_entries.iter().enumerate() {
        let layer_number = |key: &str| -> Result<f64, CloudsLuaError> {
            layer
                .field(key)
                .and_then(LuaValue::as_number)
                .ok_or_else(|| {
                    err(
                        0,
                        format!(
                            "preset `{name}`, layer {}: numeric `{key}` missing",
                            index + 1
                        ),
                    )
                })
        };
        layers.push(CloudLayer {
            altitude_min_m: layer_number("altitudeMin")?,
            altitude_max_m: layer_number("altitudeMax")?,
            coverage: layer_number("coverage")?,
        });
    }
    Ok(CloudPreset {
        readable_name: value
            .field("readableName")
            .and_then(LuaValue::as_str)
            .map(str::to_string),
        readable_name_short: value
            .field("readableNameShort")
            .and_then(LuaValue::as_str)
            .map(str::to_string),
        visible_in_gui: value.field("visibleInGUI").and_then(LuaValue::as_bool),
        precipitation_power: number("precipitationPower")?,
        preset_alt_min_m: number("presetAltMin")?,
        preset_alt_max_m: number("presetAltMax")?,
        layers,
    })
}

fn err(offset: usize, message: impl Into<String>) -> CloudsLuaError {
    CloudsLuaError {
        offset,
        message: message.into(),
    }
}

// ---------------------------------------------------------------------------------------------
// Minimal reader for the Lua subset used by DCS data files: `name = expr` statements, table
// constructors, numbers, strings, booleans, `nil`, unary minus, `..` concatenation and calls with
// a single argument such as the localisation wrapper `_('text')`. Anything else is an error.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum LuaKey {
    Name(String),
    /// `[1] = ...` or `["name"] = ...`; only string keys are looked up.
    Bracketed(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
enum LuaValue {
    Bool(bool),
    Number(f64),
    Str(String),
    /// Entries in source order; `None` key for positional entries.
    Table(Vec<(Option<LuaKey>, LuaValue)>),
    /// `nil`, or an expression this reader does not evaluate (e.g. a call on a non-string).
    Opaque,
}

impl LuaValue {
    fn field(&self, name: &str) -> Option<&LuaValue> {
        let LuaValue::Table(entries) = self else {
            return None;
        };
        entries.iter().find_map(|(key, value)| match key {
            Some(LuaKey::Name(n)) | Some(LuaKey::Bracketed(Some(n))) if n == name => Some(value),
            _ => None,
        })
    }

    fn as_number(&self) -> Option<f64> {
        match self {
            LuaValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            LuaValue::Str(s) => Some(s),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            LuaValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Number(f64),
    Str(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Eq,
    Comma,
    Semicolon,
    Minus,
    Concat,
}

struct LuaParser {
    tokens: Vec<(usize, Token)>,
    pos: usize,
}

impl LuaParser {
    fn new(text: &str) -> Result<Self, CloudsLuaError> {
        Ok(Self {
            tokens: tokenize(text)?,
            pos: 0,
        })
    }

    fn offset(&self) -> usize {
        self.tokens.get(self.pos).map(|(o, _)| *o).unwrap_or(0)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(_, t)| t)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Token> {
        self.tokens.get(self.pos + ahead).map(|(_, t)| t)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).map(|(_, t)| t.clone());
        self.pos += 1;
        token
    }

    fn expect(&mut self, expected: &Token) -> Result<(), CloudsLuaError> {
        let offset = self.offset();
        match self.next() {
            Some(ref t) if t == expected => Ok(()),
            other => Err(err(
                offset,
                format!("expected {expected:?}, found {other:?}"),
            )),
        }
    }

    /// Leading `name = expr` statements. Reading stops at the first statement of another kind
    /// (DCS data files may end with helper functions, e.g. `function deepcopy(...)`); the data
    /// tables always come first, and a missing table is reported by the caller.
    fn parse_chunk(mut self) -> Result<Vec<(String, LuaValue)>, CloudsLuaError> {
        let mut statements = Vec::new();
        while let Some(token) = self.peek().cloned() {
            match token {
                Token::Semicolon => {
                    self.next();
                }
                Token::Name(name) if self.peek_at(1) == Some(&Token::Eq) => {
                    self.next();
                    self.next();
                    let value = self.parse_expr()?;
                    statements.push((name, value));
                }
                _ => break,
            }
        }
        Ok(statements)
    }

    fn parse_expr(&mut self) -> Result<LuaValue, CloudsLuaError> {
        let mut value = self.parse_primary()?;
        while self.peek() == Some(&Token::Concat) {
            self.next();
            let rhs = self.parse_primary()?;
            value = match (value, rhs) {
                (LuaValue::Str(a), LuaValue::Str(b)) => LuaValue::Str(a + &b),
                _ => LuaValue::Opaque,
            };
        }
        Ok(value)
    }

    fn parse_primary(&mut self) -> Result<LuaValue, CloudsLuaError> {
        let offset = self.offset();
        match self.next() {
            Some(Token::Number(n)) => Ok(LuaValue::Number(n)),
            Some(Token::Str(s)) => Ok(LuaValue::Str(s)),
            Some(Token::Minus) => match self.parse_primary()? {
                LuaValue::Number(n) => Ok(LuaValue::Number(-n)),
                _ => Ok(LuaValue::Opaque),
            },
            Some(Token::LBrace) => self.parse_table(),
            Some(Token::Name(name)) => match name.as_str() {
                "true" => Ok(LuaValue::Bool(true)),
                "false" => Ok(LuaValue::Bool(false)),
                "nil" => Ok(LuaValue::Opaque),
                _ if self.peek() == Some(&Token::LParen) => {
                    self.next();
                    let arg = if self.peek() == Some(&Token::RParen) {
                        LuaValue::Opaque
                    } else {
                        self.parse_expr()?
                    };
                    self.expect(&Token::RParen)?;
                    // The localisation wrapper `_('x')` evaluates to its string.
                    Ok(match arg {
                        LuaValue::Str(s) if name == "_" => LuaValue::Str(s),
                        _ => LuaValue::Opaque,
                    })
                }
                _ => Ok(LuaValue::Opaque),
            },
            other => Err(err(offset, format!("unexpected {other:?} in expression"))),
        }
    }

    fn parse_table(&mut self) -> Result<LuaValue, CloudsLuaError> {
        let mut entries = Vec::new();
        loop {
            match self.peek() {
                Some(Token::RBrace) => {
                    self.next();
                    return Ok(LuaValue::Table(entries));
                }
                Some(Token::Comma) | Some(Token::Semicolon) => {
                    self.next();
                }
                Some(Token::LBracket) => {
                    self.next();
                    let key = match self.parse_expr()? {
                        LuaValue::Number(_) => LuaKey::Bracketed(None),
                        LuaValue::Str(s) => LuaKey::Bracketed(Some(s)),
                        _ => return Err(err(self.offset(), "unsupported table key")),
                    };
                    self.expect(&Token::RBracket)?;
                    self.expect(&Token::Eq)?;
                    let value = self.parse_expr()?;
                    entries.push((Some(key), value));
                }
                Some(Token::Name(name)) if self.peek_at(1) == Some(&Token::Eq) => {
                    let name = name.clone();
                    self.next();
                    self.next();
                    let value = self.parse_expr()?;
                    entries.push((Some(LuaKey::Name(name)), value));
                }
                Some(_) => {
                    let value = self.parse_expr()?;
                    entries.push((None, value));
                }
                None => return Err(err(self.offset(), "unterminated table")),
            }
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<(usize, Token)>, CloudsLuaError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                if let Some(level) = long_bracket_level(bytes, i) {
                    i = skip_long_bracket(bytes, i, level)
                        .ok_or_else(|| err(start, "unterminated block comment"))?;
                } else {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
            }
            b'-' => {
                tokens.push((start, Token::Minus));
                i += 1;
            }
            b'.' if bytes.get(i + 1) == Some(&b'.') => {
                tokens.push((start, Token::Concat));
                i += 2;
            }
            b'{' => {
                tokens.push((start, Token::LBrace));
                i += 1;
            }
            b'}' => {
                tokens.push((start, Token::RBrace));
                i += 1;
            }
            b'[' => {
                if let Some(level) = long_bracket_level(bytes, i) {
                    let open_len = level + 2;
                    let end = skip_long_bracket(bytes, i, level)
                        .ok_or_else(|| err(start, "unterminated long string"))?;
                    let inner = &text[i + open_len..end - open_len];
                    tokens.push((start, Token::Str(inner.to_string())));
                    i = end;
                } else {
                    tokens.push((start, Token::LBracket));
                    i += 1;
                }
            }
            b']' => {
                tokens.push((start, Token::RBracket));
                i += 1;
            }
            b'(' => {
                tokens.push((start, Token::LParen));
                i += 1;
            }
            b')' => {
                tokens.push((start, Token::RParen));
                i += 1;
            }
            b'=' => {
                tokens.push((start, Token::Eq));
                i += 1;
            }
            b',' => {
                tokens.push((start, Token::Comma));
                i += 1;
            }
            b';' => {
                tokens.push((start, Token::Semicolon));
                i += 1;
            }
            b'\'' | b'"' => {
                let quote = c;
                i += 1;
                let mut value = String::new();
                loop {
                    let Some(&b) = bytes.get(i) else {
                        return Err(err(start, "unterminated string"));
                    };
                    if b == quote {
                        i += 1;
                        break;
                    }
                    if b == b'\\' {
                        let escaped = bytes
                            .get(i + 1)
                            .copied()
                            .ok_or_else(|| err(start, "unterminated string escape"))?;
                        value.push(match escaped {
                            b'n' => '\n',
                            b't' => '\t',
                            other => other as char,
                        });
                        i += 2;
                        continue;
                    }
                    // Copy one UTF-8 character.
                    let ch_len = utf8_len(b);
                    value.push_str(&text[i..i + ch_len]);
                    i += ch_len;
                }
                tokens.push((start, Token::Str(value)));
            }
            b'0'..=b'9' | b'.' => {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.')
                    && !(bytes[i] == b'.' && bytes.get(i + 1) == Some(&b'.'))
                {
                    // Exponent sign, e.g. `1e-5`.
                    if (bytes[i] == b'e' || bytes[i] == b'E')
                        && matches!(bytes.get(i + 1), Some(b'-') | Some(b'+'))
                    {
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                let literal = &text[start..i];
                let number = literal
                    .parse::<f64>()
                    .map_err(|_| err(start, format!("bad number `{literal}`")))?;
                tokens.push((start, Token::Number(number)));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push((start, Token::Name(text[start..i].to_string())));
            }
            other => {
                return Err(err(
                    start,
                    format!("unexpected character `{}`", other as char),
                ))
            }
        }
    }
    Ok(tokens)
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Level of a long bracket `[==[` starting at `i`, if any.
fn long_bracket_level(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    let mut j = i + 1;
    while bytes.get(j) == Some(&b'=') {
        j += 1;
    }
    (bytes.get(j) == Some(&b'[')).then_some(j - i - 1)
}

/// Index just past the closing `]==]` of a long bracket opened at `i`.
fn skip_long_bracket(bytes: &[u8], i: usize, level: usize) -> Option<usize> {
    let mut j = i + level + 2;
    while j < bytes.len() {
        if bytes[j] == b']' {
            let mut k = j + 1;
            let mut eq = 0;
            while bytes.get(k) == Some(&b'=') {
                eq += 1;
                k += 1;
            }
            if eq == level && bytes.get(k) == Some(&b']') {
                return Some(k + 1);
            }
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
cloudsTechnique = 0 -- 0 - volumteric
--[[ block
comment ]]
clouds =
{
	presets =
	{
		Preset1 =
		{
			visibleInGUI = true,
			readableName = '01 ##'.._('Few Scattered Clouds \nMETAR: FEW/SCT 7/8'),
			readableNameShort = _('Light Scattered 1 [ED]'),
			precipitationPower = -1.00,
			presetAltMin = 840.00,
			presetAltMax = 4200.00,
			layers = {
				{ altitudeMin = 2520, altitudeMax = 3780, coverage = 0.359, noiseFreq = 0.384, },
				{ altitudeMin = 10500, altitudeMax = 12180, coverage = 0.000, },
			}
		},
		RainyPreset1 =
		{
			visibleInGUI = true,
			precipitationPower = 0.8,
			presetAltMin = 420,
			presetAltMax = 2940,
			layers = { { altitudeMin = 420, altitudeMax = 1260, coverage = 0.9 } },
		},
	},
}
"#;

    #[test]
    fn parses_presets_layers_and_localised_names() {
        let table = parse_clouds_lua(SAMPLE, Some("2.9.x".into())).unwrap();
        assert_eq!(table.dcs_version.as_deref(), Some("2.9.x"));
        let p1 = table.get("Preset1").unwrap();
        assert_eq!(
            p1.readable_name_short.as_deref(),
            Some("Light Scattered 1 [ED]")
        );
        assert_eq!(
            p1.readable_name.as_deref(),
            Some("01 ##Few Scattered Clouds \nMETAR: FEW/SCT 7/8")
        );
        assert_eq!(p1.visible_in_gui, Some(true));
        assert_eq!(p1.precipitation_power, -1.0);
        assert_eq!(p1.preset_alt_min_m, 840.0);
        assert_eq!(p1.layers.len(), 2);
        assert_eq!(p1.layers[0].coverage, 0.359);
        assert!(!p1.has_precipitation());
        assert!(table.get("RainyPreset1").unwrap().has_precipitation());
    }

    #[test]
    fn lowest_layer_with_coverage_skips_empty_layers() {
        let table = parse_clouds_lua(SAMPLE, None).unwrap();
        let p1 = table.get("Preset1").unwrap();
        assert_eq!(
            p1.lowest_layer_with_coverage(0.0, false)
                .unwrap()
                .altitude_min_m,
            2520.0
        );
        assert!(p1.lowest_layer_with_coverage(0.5, true).is_none());
    }

    #[test]
    fn trailing_code_after_the_data_table_is_ignored() {
        let text = format!("{SAMPLE}\n\nfunction deepcopy(orig)\n  return orig\nend\n");
        assert_eq!(parse_clouds_lua(&text, None).unwrap().presets.len(), 2);
    }

    #[test]
    fn missing_clouds_table_or_field_is_an_error() {
        assert!(parse_clouds_lua("x = 1", None).is_err());
        assert!(parse_clouds_lua("clouds = { presets = { P = { layers = {} } } }", None).is_err());
        assert!(parse_clouds_lua("clouds = { presets = { P = { ", None).is_err());
    }

    #[test]
    fn checked_in_table_parses_and_covers_the_known_preset_families() {
        let table = &*CLOUD_PRESETS;
        assert!(table.dcs_version.is_some());
        for name in [
            "Preset1",
            "Preset27",
            "RainyPreset1",
            "Preset65",
            "Preset88",
        ] {
            assert!(table.get(name).is_some(), "{name} missing from the table");
        }
        for (name, preset) in &table.presets {
            assert!(!preset.layers.is_empty(), "{name} has no layer");
            for layer in &preset.layers {
                // `coverage` is a rendering parameter, not a sky fraction: hidden presets such
                // as `clouds10` go above 1.
                assert!(
                    layer.coverage.is_finite() && layer.coverage >= 0.0,
                    "{name}: coverage {} invalid",
                    layer.coverage
                );
            }
        }
        // ED's own descriptions: an overcast-with-rain preset reads as dense and wet.
        let rainy = table.get("RainyPreset1").unwrap();
        assert_eq!(rainy.metar_cover_densities().first(), Some(&9.0));
        assert!(rainy.has_precipitation());
    }

    #[test]
    fn metar_cover_codes_skip_weather_tokens_and_average_pairs() {
        let mut preset = parse_clouds_lua(SAMPLE, None)
            .unwrap()
            .get("Preset1")
            .unwrap()
            .clone();
        assert_eq!(preset.metar_cover_densities(), vec![3.0]);
        preset.readable_name = Some("x METAR: VIS 3-5KM RA BKN/OVC LYR 3/11 SCT 18/29".into());
        assert_eq!(preset.metar_cover_densities(), vec![8.0, 4.0]);
        preset.readable_name = Some("Low clouds w/Scattered Showers".into());
        assert!(preset.metar_cover_densities().is_empty());
        preset.readable_name = Some("x METAR: --".into());
        assert!(preset.metar_cover_densities().is_empty());
    }
}
