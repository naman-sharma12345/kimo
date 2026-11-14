//! Number formatting for on-screen numbers (counters, chart labels): digit grouping in the Western (1,000,000) or Indian
//! (10,00,000) system, compact forms (1.2M / 12 L / 3.4 Cr), and optional Devanagari digits.
//!
//! Also the `{num:...}` dynamic text token (see `render::dyn_text`): a live number that animates between keyed values, so a
//! counter or a chart value label is ONE text clip instead of one clip per frame.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grouping { None, Western, Indian, Compact, CompactIndian }
impl Grouping {
    pub const NAMES: &'static [&'static str] = &["none", "western", "indian", "compact", "compact_indian"];
    pub fn parse(s: &str) -> Result<Grouping, String> {
        Ok(match s { "none" | "" => Grouping::None, "western" | "intl" | "en" => Grouping::Western, "indian" | "in" | "lakh" => Grouping::Indian, "compact" => Grouping::Compact, "compact_indian" => Grouping::CompactIndian,
            o => return Err(format!("grouping must be one of {}, got '{o}'", Grouping::NAMES.join("|"))) })
    }
}

fn group_int(digits: &str, g: Grouping, sep: char) -> String {
    let n = digits.len(); if n <= 3 || matches!(g, Grouping::None) { return digits.to_string(); }
    let mut parts: Vec<&str> = vec![]; let (head, last3) = digits.split_at(n - 3);
    match g {
        Grouping::Indian | Grouping::CompactIndian => { let mut h = head; while h.len() > 2 { let (a, b) = h.split_at(h.len() - 2); parts.push(b); h = a; } if !h.is_empty() { parts.push(h); } parts.reverse(); }
        _ => { let mut h = head; while h.len() > 3 { let (a, b) = h.split_at(h.len() - 3); parts.push(b); h = a; } if !h.is_empty() { parts.push(h); } parts.reverse(); }
    }
    parts.push(last3); parts.join(&sep.to_string())
}

/// Format `v` with `decimals` fraction digits and the chosen grouping. Compact forms pick a unit (K/M/B, or L/Cr) and keep
/// `decimals` digits after it (trailing zeros dropped).
pub fn format(v: f64, decimals: usize, g: Grouping) -> String {
    if !v.is_finite() { return "-".into(); }
    let decimals = decimals.min(6);
    let (val, unit) = match g {
        Grouping::Compact => { let a = v.abs(); if a >= 1e12 { (v / 1e12, "T") } else if a >= 1e9 { (v / 1e9, "B") } else if a >= 1e6 { (v / 1e6, "M") } else if a >= 1e3 { (v / 1e3, "K") } else { (v, "") } }
        Grouping::CompactIndian => { let a = v.abs(); if a >= 1e7 { (v / 1e7, " Cr") } else if a >= 1e5 { (v / 1e5, " L") } else if a >= 1e3 { (v / 1e3, "K") } else { (v, "") } }
        _ => (v, ""),
    };
    let s = format!("{:.*}", decimals, val.abs());
    let (int, frac) = match s.split_once('.') { Some((a, b)) => (a.to_string(), Some(b.to_string())), None => (s.clone(), None) };
    let neg = val < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0');
    let int = match g { Grouping::Compact | Grouping::CompactIndian => int, _ => group_int(&int, g, ',') };
    let compact = matches!(g, Grouping::Compact | Grouping::CompactIndian);
    let frac = match (frac, !compact) { (Some(f), false) => { let f = f.trim_end_matches('0').to_string(); if f.is_empty() { None } else { Some(f) } } (f, _) => f };
    let mut o = String::new(); if neg { o.push('-'); } o += &int; if let Some(f) = frac { o.push('.'); o += &f; } o += unit; o
}

/// Replace ASCII digits with Devanagari digits (U+0966..U+096F).
pub fn devanagari_digits(s: &str) -> String { s.chars().map(|c| if c.is_ascii_digit() { char::from_u32(0x0966 + (c as u32 - '0' as u32)).unwrap() } else { c }).collect() }

/// Ease used between `{num}` keys.
fn ease(u: f64, e: &str) -> f64 { let u = u.clamp(0.0, 1.0); match e { "linear" => u, "out" => 1.0 - (1.0 - u).powi(3), "in_out" => if u < 0.5 { 4.0 * u * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(3) / 2.0 }, "hold" => 0.0, _ => 1.0 - (1.0 - u).powi(4) } }

/// Value of a keyed number at time `t`: keys are (time, value) sorted by time; holds before the first and after the last.
pub fn value_at(keys: &[(f64, f64)], t: f64, ease_name: &str) -> f64 {
    if keys.is_empty() { return 0.0; } if t <= keys[0].0 { return keys[0].1; } if t >= keys[keys.len() - 1].0 { return keys[keys.len() - 1].1; }
    let i = keys.windows(2).position(|w| t >= w[0].0 && t < w[1].0).unwrap_or(0); let (a, b) = (keys[i], keys[i + 1]);
    a.1 + (b.1 - a.1) * ease((t - a.0) / (b.0 - a.0).max(1e-9), ease_name)
}

/// Build a `{num:...}` token. Body grammar: `num:DECIMALS:GROUPING:EASE:t0=v0,t1=v1,...[:deva]`.
pub fn token(decimals: usize, g: &str, ease_name: &str, keys: &[(f64, f64)], devanagari: bool) -> String {
    let k: Vec<String> = keys.iter().map(|(t, v)| format!("{}={}", trim(*t), trim(*v))).collect();
    format!("{{num:{decimals}:{g}:{ease_name}:{}{}}}", k.join(","), if devanagari { ":deva" } else { "" })
}
fn trim(v: f64) -> String { let s = format!("{:.6}", v); s.trim_end_matches('0').trim_end_matches('.').to_string() }

/// Resolve the body of a `{num:...}` token (without braces) at time `t`. None if it is malformed (shown as written).
pub fn resolve(body: &str, t: f64) -> Option<String> {
    let p: Vec<&str> = body.split(':').collect(); if p.len() < 5 || p[0] != "num" { return None; }
    let dec: usize = p[1].parse().ok()?; let g = Grouping::parse(p[2]).ok()?; let e = p[3];
    let mut keys: Vec<(f64, f64)> = vec![]; for kv in p[4].split(',') { let (a, b) = kv.split_once('=')?; keys.push((a.trim().parse().ok()?, b.trim().parse().ok()?)); }
    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let s = format(value_at(&keys, t, e), dec, g); Some(if p.get(5) == Some(&"deva") { devanagari_digits(&s) } else { s })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indian_grouping() {
        assert_eq!(format(100000.0, 0, Grouping::Indian), "1,00,000");
        assert_eq!(format(12345678.0, 0, Grouping::Indian), "1,23,45,678");
        assert_eq!(format(1234567890.5, 1, Grouping::Indian), "1,23,45,67,890.5");
        assert_eq!(format(999.0, 0, Grouping::Indian), "999");
        assert_eq!(format(1000.0, 0, Grouping::Indian), "1,000");
        assert_eq!(format(-250000.0, 2, Grouping::Indian), "-2,50,000.00");
    }
    #[test]
    fn western_and_none() {
        assert_eq!(format(1234567.0, 0, Grouping::Western), "1,234,567");
        assert_eq!(format(1234567.0, 0, Grouping::None), "1234567");
        assert_eq!(format(-0.001, 0, Grouping::Western), "0", "no negative zero");
    }
    #[test]
    fn compact_forms() {
        assert_eq!(format(1_260_000.0, 1, Grouping::Compact), "1.3M");
        assert_eq!(format(2_000_000_000.0, 1, Grouping::Compact), "2B");
        assert_eq!(format(1_500_000.0, 1, Grouping::CompactIndian), "15 L");
        assert_eq!(format(32_000_000.0, 1, Grouping::CompactIndian), "3.2 Cr");
        assert_eq!(format(950.0, 0, Grouping::CompactIndian), "950");
        assert_eq!(format(0.0, 1, Grouping::Compact), "0", "compact forms drop trailing zeros"); assert_eq!(format(2.5, 2, Grouping::Compact), "2.5");
    }
    #[test]
    fn devanagari() { assert_eq!(devanagari_digits("1,00,000"), "१,००,०००"); }
    #[test]
    fn num_token_animates_between_keys() {
        let tok = token(0, "indian", "linear", &[(1.0, 0.0), (2.0, 200000.0)], false);
        let body = &tok[1..tok.len() - 1];
        assert_eq!(resolve(body, 0.0).unwrap(), "0"); assert_eq!(resolve(body, 1.5).unwrap(), "1,00,000"); assert_eq!(resolve(body, 9.0).unwrap(), "2,00,000");
        assert_eq!(resolve("num:0:indian:linear:0=5:deva", 0.0).unwrap(), "५");
        assert!(resolve("num:x:indian:linear:0=5", 0.0).is_none()); assert!(resolve("num:0:martian:linear:0=5", 0.0).is_none());
    }
}
