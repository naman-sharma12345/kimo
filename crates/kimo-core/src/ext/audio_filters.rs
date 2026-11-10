//! Literal coefficient filters and analytic response. No creative normalization.
use super::kit::*;
use crate::editorial::AudioEffect;
fn nums(v: &Value) -> Result<Vec<f64>, String> {
    v.as_array()
        .ok_or("invalid_filter: coefficient array required")?
        .iter()
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid_filter: finite coefficients required".into())
        })
        .collect()
}
pub fn parse_effect(kind: &str, p: &Value) -> Result<AudioEffect, String> {
    if !p.is_object() {
        return Err("invalid_filter: params object required".into());
    }
    let allowed: &[&str] = match kind {
        "biquad" => &["sections", "channels", "rate", "freqs", "n", "fmin", "fmax"],
        "fir" => &["coeffs", "normalize", "rate", "freqs", "n", "fmin", "fmax"],
        "mix_matrix" => &["matrix", "out_channels"],
        _ => &[],
    };
    for k in p.as_object().unwrap().keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("invalid_filter: unknown {kind} parameter {k}"));
        }
    }
    match kind {
        "biquad" => {
            let arr = p
                .get("sections")
                .map(|v| {
                    v.as_array()
                        .ok_or("invalid_filter: sections array required")
                })
                .transpose()?;
            let mut sections = vec![];
            for s in arr.into_iter().flatten() {
                let map = s
                    .as_object()
                    .ok_or("invalid_filter: section object required")?;
                for k in map.keys() {
                    if !["b0", "b1", "b2", "a1", "a2"].contains(&k.as_str()) {
                        return Err(format!(
                            "invalid_filter: unknown raw coefficient {k}; a0 fixed1"
                        ));
                    }
                }

                let f = |k: &str, d: f64| -> Result<f64, String> {
                    s.get(k)
                        .map(|v| {
                            v.as_f64().filter(|v| v.is_finite()).ok_or(
                                "invalid_filter: finite section coefficients required".into(),
                            )
                        })
                        .transpose()
                        .map(|v| v.unwrap_or(d))
                };
                sections.push([
                    f("b0", 1.)?,
                    f("b1", 0.)?,
                    f("b2", 0.)?,
                    f("a1", 0.)?,
                    f("a2", 0.)?,
                ]);
            }
            if sections.len() > 1024 {
                return Err("out_of_range: max1024 biquad sections".into());
            }
            if sections.is_empty() {
                sections.push([1., 0., 0., 0., 0.]);
            }
            let channels = p
                .get("channels")
                .map(|v| {
                    v.as_array()
                        .ok_or("invalid_filter: channels array required")?
                        .iter()
                        .map(|v| {
                            v.as_u64()
                                .filter(|v| *v < 2)
                                .map(|v| v as usize)
                                .ok_or("out_of_range: decoded channels0,1")
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            Ok(AudioEffect::Biquad {
                sections,
                channels,
                keys: Default::default(),
            })
        }
        "fir" => {
            if p.get("normalize").and_then(Value::as_bool) == Some(true) {
                return Err("invalid_filter: normalize must be false".into());
            }
            let coeffs = p.get("coeffs").map(nums).transpose()?.unwrap_or(vec![1.]);
            if coeffs.is_empty() || coeffs.len() > 65536 {
                return Err("out_of_range: fir coefficients1..65536".into());
            }
            Ok(AudioEffect::Fir {
                coeffs,
                keys: Default::default(),
            })
        }
        "mix_matrix" => {
            let matrix = p
                .get("matrix")
                .map(|v| {
                    v.as_array()
                        .ok_or("invalid_filter: matrix array required")?
                        .iter()
                        .map(nums)
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or(vec![vec![1., 0.], vec![0., 1.]]);
            if matrix.len() != 2
                || matrix.iter().any(|r| r.len() != 2)
                || p.get("out_channels")
                    .and_then(Value::as_u64)
                    .map_or(false, |n| n != 2)
            {
                return Err(
                    "invalid_filter: fixed stereo mixer requires2x2 matrix/out_channels2".into(),
                );
            }
            Ok(AudioEffect::MixMatrix {
                matrix,
                keys: Default::default(),
            })
        }
        _ => Err("unknown raw audio effect".into()),
    }
}
fn response(_ss: &mut Session, a: &Value) -> Result<Value, String> {
    let rate = a.get("rate").and_then(Value::as_f64).unwrap_or(48000.);
    if !rate.is_finite() || rate < 1000. || rate > 384000. {
        return Err("out_of_range: rate1000..384000".into());
    }
    if a.get("sections").is_some() && a.get("coeffs").is_some() {
        return Err("invalid_filter: choose sections or coeffs".into());
    }
    let effect = if a.get("coeffs").is_some() {
        parse_effect("fir", a)?
    } else {
        parse_effect("biquad", a)?
    };
    let freqs = if let Some(f) = a.get("freqs") {
        nums(f)?
    } else {
        let n = a.get("n").and_then(Value::as_u64).unwrap_or(128) as usize;
        if n == 0 || n > 65536 {
            return Err("out_of_range: n1..65536".into());
        }
        let min = a.get("fmin").and_then(Value::as_f64).unwrap_or(20.);
        let max = a
            .get("fmax")
            .and_then(Value::as_f64)
            .unwrap_or(rate / 2. - 1.);
        (0..n)
            .map(|i| min + (max - min) * i as f64 / (n - 1).max(1) as f64)
            .collect()
    };
    let mut out = vec![];
    for f in freqs {
        if !f.is_finite() || f < 0. || f >= rate / 2. {
            return Err("out_of_range: frequencies0<=f<Nyquist".into());
        }
        let w = std::f64::consts::TAU * f / rate;
        let eval = |c: &[f64]| -> (f64, f64) {
            c.iter().enumerate().fold((0., 0.), |(r, i), (k, c)| {
                (r + c * (w * k as f64).cos(), i - c * (w * k as f64).sin())
            })
        };
        let (mut re, mut im) = (1., 0.);
        match &effect {
            AudioEffect::Biquad { sections, .. } => {
                for s in sections {
                    let (nr, ni) = eval(&s[..3]);
                    let (dr, di) = eval(&[1., s[3], s[4]]);
                    let d = dr * dr + di * di;
                    if d == 0. {
                        return Err("invalid_filter: response singularity".into());
                    }
                    let (r, i) = ((nr * dr + ni * di) / d, (ni * dr - nr * di) / d);
                    (re, im) = (re * r - im * i, re * i + im * r);
                }
            }
            AudioEffect::Fir { coeffs, .. } => (re, im) = eval(coeffs),
            _ => unreachable!(),
        }
        let mag = (re * re + im * im).sqrt();
        out.push(json!({"freq":f,"magnitude":mag,"magnitude_db":if mag==0.{None}else{Some(20.*mag.log10())},"phase_rad":im.atan2(re)}));
    }
    Ok(
        json!({"rate":rate,"response":out,"zero_magnitude_db":"null represents -infinity","state":"none, analytic only"}),
    )
}
fn describe(_ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = str_arg(a, "kind")?;
    let formula=match kind{"biquad"=>"DF-II transposed: y=b0*x+s1; s1=b1*x-a1*y+s2; s2=b2*x-a2*y; a0=1, f64 state, f32 buffers. Raw coefficients unrestricted finite; no freq/Q convenience. Instability may overflow, rejected not substituted.","fir"=>"y[n]=sum(coeffs[k]*x[n-k]); zero prehistory, fixed output length; no normalization or clamp.","mix_matrix"=>"out[o,n]=sum(matrix[o,i]*in[i,n]); f64 accumulation into f32, unclamped headroom. Fixed stereo2x2.",_=>return Err("unknown kind; valid: biquad, fir, mix_matrix".into())};
    Ok(
        json!({"kind":kind,"formula":formula,"default":"identity","stage":"post_filter before volume/pan/duck/mix","coefficient_keying":"linear clip-local coefficient keys via set_audio_effect_key; instability not guarded except nonfinite rejection","normalization":false}),
    )
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"set_audio_effect_key",mutates:true,description:"Set a linear clip-local key on raw audio coefficients. sections.N.b0/b1/b2/a1/a2, coeffs.N, matrix.OUT.IN. Finite literal values only, feedback instability unguarded except nonfinite-output rejection. No silent clamp.",schema:obj(vec![("clip_id",s("media clip")),("effect_index",n("zero-based effect")),("coefficient",s("raw coefficient path")),("time",n("clip-local seconds")),("value",n("finite coefficient"))],&["clip_id","effect_index","coefficient","time","value"]),run:key},Command{name:"audio_filter_response",mutates:false,description:"Pure analytic raw biquad sections or FIR coeffs response, frequencies0<=f<Nyquist. Magnitude dB/linear and phase radians, zero gain dB null=-infinity. No state/sample mutation. a0=1, finite literal coefficients, no creative normalization.",schema:obj(vec![("sections",json!({"type":"array"})),("coeffs",json!({"type":"array"})),("freqs",json!({"type":"array"})),("n",n("frequency point count")),("fmin",n("Hz")),("fmax",n("Hz")),("rate",n("Hz default48000"))],&[]),run:response},Command{name:"audio_effect_describe",mutates:false,description:"Raw audio effect formulas, defaults, state precision, unclamped headroom and limits. Literal coefficients, linear clip-local keying, unstable feedback not silently clamped.",schema:obj(vec![("kind",json!({"type":"string","enum":["biquad","fir","mix_matrix"]}))],&["kind"]),run:describe}]
}

fn key(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "clip_id")?;
    let effect = a
        .get("effect_index")
        .and_then(Value::as_u64)
        .ok_or("missing integer 'effect_index'")? as usize;
    let coefficient = str_arg(a, "coefficient")?;
    let time = num_arg(a, "time")?;
    let value = num_arg(a, "value")?;
    if !time.is_finite() || time < 0. || !value.is_finite() {
        return Err("invalid_filter: finite coefficient and nonnegative time required".into());
    }
    let clip = ss.project.find_clip_mut(id).ok_or("no such clip")?;
    let e = clip
        .audio
        .effects
        .get_mut(effect)
        .ok_or("invalid_filter: effect_index not found")?;
    let parts: Vec<_> = coefficient.split('.').collect();
    let (keys, base) = match e {
        AudioEffect::Biquad { sections, keys, .. } => {
            if parts.len() != 3 || parts[0] != "sections" {
                return Err("invalid_filter: coefficient sections.N.b0|b1|b2|a1|a2".into());
            }
            let n = parts[1]
                .parse::<usize>()
                .ok()
                .filter(|&n| n < sections.len())
                .ok_or("invalid_filter: section index invalid")?;
            let k = ["b0", "b1", "b2", "a1", "a2"]
                .iter()
                .position(|&k| k == parts[2])
                .ok_or("invalid_filter: coefficient name invalid")?;
            (keys, sections[n][k])
        }
        AudioEffect::Fir { coeffs, keys } => {
            if parts.len() != 2 || parts[0] != "coeffs" {
                return Err("invalid_filter: coefficient coeffs.N".into());
            }
            let n = parts[1]
                .parse::<usize>()
                .ok()
                .filter(|&n| n < coeffs.len())
                .ok_or("invalid_filter: tap invalid")?;
            (keys, coeffs[n])
        }
        AudioEffect::MixMatrix { matrix, keys } => {
            if parts.len() != 3 || parts[0] != "matrix" {
                return Err("invalid_filter: coefficient matrix.OUT.IN".into());
            }
            let o = parts[1]
                .parse::<usize>()
                .ok()
                .filter(|&n| n < 2)
                .ok_or("invalid_filter: output channel invalid")?;
            let i = parts[2]
                .parse::<usize>()
                .ok()
                .filter(|&n| n < 2)
                .ok_or("invalid_filter: input channel invalid")?;
            (keys, matrix[o][i])
        }
        _ => return Err("invalid_filter: raw audio effect required".into()),
    };
    keys.entry(coefficient.into())
        .or_insert_with(|| crate::keyframe::Prop::new(base))
        .set_key(time, value, crate::keyframe::Ease::Linear);
    Ok(
        json!({"ok":true,"coefficient":coefficient,"interpolation":"linear","instability":"unguarded except non-finite output rejection"}),
    )
}
