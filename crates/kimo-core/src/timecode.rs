//! Explicit SMPTE label conversion, non-drop and 29.97/59.94 drop-frame.
pub fn seconds(label: &str, fps: f64, drop: bool) -> Result<f64, String> {
    let delim = if drop { ';' } else { ':' };
    let mut parts = label.rsplitn(2, delim);
    let frames = parts.next().ok_or("missing frame number")?;
    let clock = parts
        .next()
        .ok_or("expected HH:MM:SS;FF (drop) or HH:MM:SS:FF")?;
    let mut nums: Vec<u64> = clock
        .split(':')
        .map(|x| x.parse::<u64>().map_err(|_| "invalid timecode".to_string()))
        .collect::<Result<_, _>>()?;
    nums.push(frames.parse().map_err(|_| "invalid timecode frame")?);
    if nums.len() != 4 || nums[0] > 23 || nums[1] > 59 || nums[2] > 59 {
        return Err("timecode clock out of range".into());
    }
    let (nominal, skip, rate) = if drop {
        if (fps - 30000. / 1001.).abs() < 0.001 {
            (30u64, 2u64, 30000. / 1001.)
        } else if (fps - 60000. / 1001.).abs() < 0.001 {
            (60, 4, 60000. / 1001.)
        } else {
            return Err("drop-frame needs 30000/1001 or 60000/1001 fps".into());
        }
    } else {
        if !fps.is_finite() || fps.fract() != 0. || !(1. ..=120.).contains(&fps) {
            return Err("non-drop needs integer fps".into());
        }
        (fps as u64, 0, fps)
    };
    if nums[3] >= nominal {
        return Err("timecode frame out of range".into());
    }
    if drop && nums[1] % 10 != 0 && nums[2] == 0 && nums[3] < skip {
        return Err("skipped drop-frame label does not exist".into());
    }
    let minutes = nums[0] * 60 + nums[1];
    let frames = ((nums[0] * 3600 + nums[1] * 60 + nums[2]) * nominal + nums[3])
        - skip * (minutes - minutes / 10);
    Ok(frames as f64 / rate)
}

/// Format a seconds offset as an SMPTE label -- the inverse of `seconds`, same fps/drop constraints.
/// Uses the standard drop-frame encode (skip 2 frames at the start of each minute except every 10th,
/// or 4 for 59.94): a nominal frame count is recovered from the real elapsed frame count by adding back
/// the frames that were skipped along the way, then split into HH:MM:SS:FF.
pub fn label(t: f64, fps: f64, drop: bool) -> Result<String, String> {
    if !t.is_finite() || t < 0.0 {
        return Err("timecode seconds must be >= 0".into());
    }
    let (nominal, skip, rate, delim): (i64, i64, f64, char) = if drop {
        if (fps - 30000. / 1001.).abs() < 0.001 {
            (30, 2, 30000. / 1001., ';')
        } else if (fps - 60000. / 1001.).abs() < 0.001 {
            (60, 4, 60000. / 1001., ';')
        } else {
            return Err("drop-frame needs 30000/1001 or 60000/1001 fps".into());
        }
    } else {
        if !fps.is_finite() || fps.fract() != 0. || !(1. ..=120.).contains(&fps) {
            return Err("non-drop needs integer fps".into());
        }
        (fps as i64, 0, fps, ':')
    };
    let frame_count = (t * rate).round() as i64;
    let total = if drop {
        let frames_per_10min = nominal * 60 * 10 - skip * 9;
        let frames_per_min = nominal * 60 - skip;
        let d = frame_count / frames_per_10min;
        let m = frame_count % frames_per_10min;
        let extra = if m > skip { skip * 9 * d + skip * ((m - skip) / frames_per_min) } else { skip * 9 * d };
        frame_count + extra
    } else {
        frame_count
    };
    let ff = total % nominal;
    let total_s = total / nominal;
    let ss = total_s % 60;
    let total_m = total_s / 60;
    let mm = total_m % 60;
    let hh = (total_m / 60) % 24;
    Ok(format!("{hh:02}:{mm:02}:{ss:02}{delim}{ff:02}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundaries() {
        let r = 30000. / 1001.;
        assert!((seconds("00:01:00;02", r, true).unwrap() - 60.06).abs() < 1e-9);
        assert!((seconds("00:10:00;00", r, true).unwrap() - 599.9994).abs() < 1e-9);
        assert!((seconds("01:00:00;00", r, true).unwrap() - 3599.9964).abs() < 1e-9);
        assert!(seconds("00:01:00;00", r, true).is_err());
        assert!(seconds("00:01:00;01", r, true).is_err());
        assert!(seconds("00:01:00;03", 60000. / 1001., true).is_err());
        assert!(seconds("00:01:00;04", 60000. / 1001., true).is_ok());
        assert!(seconds("00:10:00;00", 60000. / 1001., true).is_ok());
        assert!(seconds("01:00:00:00", 25., false).unwrap() == 3600.);
        assert!(seconds("00:10:00;00", 25., true).is_err());
        assert!(seconds("00:10:00:00", r, true).is_err());
        }

    #[test]
    fn label_matches_known_boundaries() {
        let r = 30000. / 1001.;
        assert_eq!(label(60.06, r, true).unwrap(), "00:01:00;02");
        assert_eq!(label(599.9994, r, true).unwrap(), "00:10:00;00");
        assert_eq!(label(3599.9964, r, true).unwrap(), "01:00:00;00");
        assert_eq!(label(3600., 25., false).unwrap(), "01:00:00:00");
        assert_eq!(label(0.0, r, true).unwrap(), "00:00:00;00");
        assert_eq!(label(0.0, 60000. / 1001., true).unwrap(), "00:00:00;00");
        assert!(label(-1.0, 25., false).is_err());
        assert!(label(1.0, 25.0001, false).is_err(), "non-integer fps rejected same as seconds()");
    }

    #[test]
    fn label_and_seconds_round_trip() {
        // Every non-skipped label this file's own `seconds` parser accepts round-trips through `label`
        // back to the identical string (frame-exact), for both drop-frame rates and a plain integer fps.
        for (fps, drop) in [(30000. / 1001., true), (60000. / 1001., true), (25.0, false), (24.0, false)] {
            let fps: f64 = fps;
            let nominal: u64 = if drop { if (fps - 30000. / 1001.).abs() < 0.001 { 30 } else { 60 } } else { fps as u64 };
            let mut n = 0usize;
            for h in 0..2u64 {
                for m in [0u64, 1, 9, 10, 11, 59] {
                    for s in [0u64, 30, 59] {
                        for f in [0u64, nominal / 2, nominal - 1] {
                            let delim = if drop { ';' } else { ':' };
                            let l = format!("{h:02}:{m:02}:{s:02}{delim}{f:02}");
                            if let Ok(t) = seconds(&l, fps, drop) {
                                assert_eq!(label(t, fps, drop).unwrap(), l, "round trip at fps={fps} drop={drop}");
                                n += 1;
                            }
                        }
                    }
                }
            }
            assert!(n > 20, "too few labels exercised for fps={fps} drop={drop}: {n}");
        }
    }
}
