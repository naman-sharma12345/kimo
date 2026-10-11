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
}
