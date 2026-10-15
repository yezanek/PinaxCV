//! Typing dates: "2021", "2021-03", "03/2021" or "3/2021" (spec section 4.2).

use cv_core::PartialDate;

/// Parses a year, or a year and month. Returns `None` for anything else,
/// including months outside 1..=12 and years outside 1900..=2100.
pub fn parse(text: &str) -> Option<PartialDate> {
    let t = text.trim();
    let (year, month) = if let Some((y, m)) = t.split_once('-') {
        (y, Some(m))
    } else if let Some((m, y)) = t.split_once('/') {
        (y, Some(m))
    } else {
        (t, None)
    };
    if year.len() != 4 {
        return None;
    }
    let year: u16 = year.parse().ok().filter(|y| (1900..=2100).contains(y))?;
    let month = match month {
        Some(m) => Some(m.parse::<u8>().ok().filter(|m| (1..=12).contains(m))?),
        None => None,
    };
    Some(PartialDate { year, month })
}

/// The text form `parse` accepts: "2021" or "2021-03".
pub fn format(date: Option<PartialDate>) -> String {
    match date {
        None => String::new(),
        Some(PartialDate { year, month: None }) => year.to_string(),
        Some(PartialDate {
            year,
            month: Some(m),
        }) => format!("{year}-{m:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(year: u16, month: Option<u8>) -> Option<PartialDate> {
        Some(PartialDate { year, month })
    }

    #[test]
    fn parses_common_forms() {
        assert_eq!(parse("2021"), d(2021, None));
        assert_eq!(parse(" 2021-03 "), d(2021, Some(3)));
        assert_eq!(parse("03/2021"), d(2021, Some(3)));
        assert_eq!(parse("3/2021"), d(2021, Some(3)));
        for bad in [
            "",
            "2",
            "202",
            "21",
            "2021-13",
            "2021-0",
            "13/2021",
            "x/2021",
            "1800",
            "2021-03-01",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn formats_round_trip() {
        for text in ["2021", "2021-03"] {
            assert_eq!(format(parse(text)), text);
        }
        assert_eq!(format(None), "");
    }
}
