//! Timed text and synchronized lyrics parsing for Apple Music TTML.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricLine {
    pub time_secs: u64,
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
}

impl LyricLine {
    pub fn new(start_secs: f64, end_secs: f64, text: impl Into<String>) -> Self {
        Self {
            time_secs: start_secs as u64,
            start_secs,
            end_secs,
            text: text.into(),
        }
    }
}

/// Parse a time string in `ss.ms`, `mm:ss.ms`, or `hh:mm:ss.ms` format into fractional seconds.
pub fn parse_ttml_timestamp(s: &str) -> Option<f64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => parts[0].parse::<f64>().ok(),
        2 => {
            let mins = parts[0].parse::<f64>().ok()?;
            let secs = parts[1].parse::<f64>().ok()?;
            Some(mins * 60.0 + secs)
        }
        3 => {
            let hours = parts[0].parse::<f64>().ok()?;
            let mins = parts[1].parse::<f64>().ok()?;
            let secs = parts[2].parse::<f64>().ok()?;
            Some(hours * 3600.0 + mins * 60.0 + secs)
        }
        _ => None,
    }
}

/// Strip any nested XML tags (e.g., `<span>...</span>`).
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}

/// Unescape standard XML and HTML entities.
pub fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Parse Apple Music TTML (Timed Text Markup Language) XML into sorted `LyricLine`s.
pub fn parse_ttml(xml: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    let mut rest = xml;

    while let Some(p_start) = rest.find("<p ") {
        rest = &rest[p_start + 3..];
        let Some(tag_end) = rest.find('>') else { break };
        let tag_attrs = &rest[..tag_end];
        let content_and_beyond = &rest[tag_end + 1..];

        let Some(p_close) = content_and_beyond.find("</p>") else {
            break;
        };
        let inner_content = &content_and_beyond[..p_close];
        rest = &content_and_beyond[p_close + 4..];

        // Parse begin
        let Some(begin_idx) = tag_attrs.find("begin=\"") else {
            continue;
        };
        let begin_val = &tag_attrs[begin_idx + 7..];
        let Some(begin_end) = begin_val.find('"') else {
            continue;
        };
        let begin_str = &begin_val[..begin_end];
        let Some(start_secs) = parse_ttml_timestamp(begin_str) else {
            continue;
        };

        // Parse end (optional, default to start_secs + 3.5s)
        let end_secs = if let Some(end_idx) = tag_attrs.find("end=\"") {
            let end_val = &tag_attrs[end_idx + 5..];
            if let Some(end_end) = end_val.find('"') {
                parse_ttml_timestamp(&end_val[..end_end]).unwrap_or(start_secs + 3.5)
            } else {
                start_secs + 3.5
            }
        } else {
            start_secs + 3.5
        };

        let clean_text = unescape_xml(&strip_tags(inner_content));
        let trimmed = clean_text.trim();
        if !trimmed.is_empty() {
            lines.push(LyricLine::new(start_secs, end_secs, trimmed));
        }
    }

    lines.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ttml_timestamps() {
        assert_eq!(parse_ttml_timestamp("27.395"), Some(27.395));
        assert_eq!(parse_ttml_timestamp("1:00.964"), Some(60.964));
        assert_eq!(parse_ttml_timestamp("2:19.610"), Some(139.610));
        assert_eq!(parse_ttml_timestamp("01:02:03.500"), Some(3723.5));
        assert_eq!(parse_ttml_timestamp("invalid"), None);
    }

    #[test]
    fn test_parse_ttml_sample() {
        let sample = r#"
            <tt xmlns="http://www.w3.org/ns/ttml">
                <body>
                    <div begin="27.395" end="48.621">
                        <p begin="27.395" end="28.960" itunes:key="L1">I been tryna call</p>
                        <p begin="30.189" end="32.529" itunes:key="L2">I&#39;ve been on my own</p>
                        <p begin="1:00.964" end="1:06.652">I said, ooh, I&#39;m <span>blinded</span></p>
                    </div>
                </body>
            </tt>
        "#;
        let lines = parse_ttml(sample);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "I been tryna call");
        assert_eq!(lines[0].start_secs, 27.395);
        assert_eq!(lines[0].end_secs, 28.960);
        assert_eq!(lines[0].time_secs, 27);

        assert_eq!(lines[1].text, "I've been on my own");
        assert_eq!(lines[1].start_secs, 30.189);

        assert_eq!(lines[2].text, "I said, ooh, I'm blinded");
        assert_eq!(lines[2].start_secs, 60.964);
        assert_eq!(lines[2].end_secs, 66.652);
    }
}
