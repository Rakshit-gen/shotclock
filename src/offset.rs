//! Clock offsets as people type them: "+1h23m", "-5h37m", "90s", "-0:30".
//! An offset is what to add to the camera's time to get the true time.

/// Parse an offset into seconds.
pub fn parse(s: &str) -> Result<i64, String> {
    let err = || {
        format!("can't read \"{s}\" as an offset; write it like +1h23m, -45m, 30s or -2:15")
    };
    let t = s.trim();
    let (sign, rest) = match t.as_bytes().first() {
        Some(b'-') => (-1, &t[1..]),
        Some(b'+') => (1, &t[1..]),
        _ => (1, t),
    };
    let rest = rest.trim().to_ascii_lowercase();
    if rest.is_empty() {
        return Err(err());
    }
    // h:mm or h:mm:ss
    if rest.contains(':') {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() > 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|c| c.is_ascii_digit())) {
            return Err(err());
        }
        let nums: Vec<i64> = parts.iter().map(|p| p.parse().unwrap_or(0)).collect();
        let secs = nums[0] * 3600 + nums[1] * 60 + nums.get(2).copied().unwrap_or(0);
        return Ok(sign * secs);
    }
    let mut total = 0i64;
    let mut num = String::new();
    let mut last_unit = u8::MAX;
    for c in rest.chars().filter(|c| !c.is_whitespace()) {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let (rank, mult) = match c {
            'd' => (0, 86_400),
            'h' => (1, 3600),
            'm' => (2, 60),
            's' => (3, 1),
            _ => return Err(err()),
        };
        // Units must come largest first, each once: "1h30m", not "30m1h".
        if num.is_empty() || (last_unit != u8::MAX && rank <= last_unit) {
            return Err(err());
        }
        last_unit = rank;
        total += num.parse::<i64>().map_err(|_| err())? * mult;
        num.clear();
    }
    if !num.is_empty() {
        // A bare number is minutes, the unit clock errors are usually in.
        if last_unit != u8::MAX {
            return Err(err());
        }
        total = num.parse::<i64>().map_err(|_| err())? * 60;
    }
    Ok(sign * total)
}

/// Seconds as "+5h37m", "-1h", "+12m30s" or "0".
pub fn format(secs: i64) -> String {
    if secs == 0 {
        return "0".into();
    }
    let sign = if secs < 0 { '-' } else { '+' };
    let a = secs.unsigned_abs();
    let (d, h, m, s) = (a / 86_400, a / 3600 % 24, a / 60 % 60, a % 60);
    let mut out = sign.to_string();
    for (v, u) in [(d, 'd'), (h, 'h'), (m, 'm'), (s, 's')] {
        if v > 0 {
            out += &format!("{v}{u}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_ways_people_write_offsets() {
        assert_eq!(parse("+1h23m"), Ok(4980));
        assert_eq!(parse("-5h37m"), Ok(-(5 * 3600 + 37 * 60)));
        assert_eq!(parse("90s"), Ok(90));
        assert_eq!(parse("45"), Ok(2700));
        assert_eq!(parse("-0:30"), Ok(-1800));
        assert_eq!(parse("1:02:03"), Ok(3723));
        assert_eq!(parse(" + 1H 5M "), Ok(3900));
        assert_eq!(parse("2d"), Ok(172_800));
        for bad in ["", "+", "1x", "30m1h", "1h1h", "1h30", "1::2", "abc"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn formats_round_trip() {
        for s in [0, 1, 59, 60, 3600, -20220, 4980, 90061, -3599] {
            assert_eq!(parse(&format(s)), Ok(s), "{}", format(s));
        }
        assert_eq!(format(-20220), "-5h37m");
    }
}
