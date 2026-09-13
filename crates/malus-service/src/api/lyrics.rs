//! TTML lyrics parsing for Apple Music.

use malus_model::{LyricLine, LyricSyllable, Lyrics};

/// Parse a time string in format `[HH:]MM:SS[.mmm]` or `SS[.mmm]` into milliseconds.
pub fn parse_ttml_time(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.len() {
            2 => {
                let mins: f64 = parts[0].parse().ok()?;
                let secs: f64 = parts[1].parse().ok()?;
                Some(((mins * 60.0 + secs) * 1000.0).round() as u64)
            }
            3 => {
                let hours: f64 = parts[0].parse().ok()?;
                let mins: f64 = parts[1].parse().ok()?;
                let secs: f64 = parts[2].parse().ok()?;
                Some(((hours * 3600.0 + mins * 60.0 + secs) * 1000.0).round() as u64)
            }
            _ => None,
        }
    } else {
        let secs: f64 = s.parse().ok()?;
        Some((secs * 1000.0).round() as u64)
    }
}

/// Decode basic XML entities.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Strip XML tags from a string and decode entities.
fn strip_tags_and_decode(s: &str) -> String {
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
    decode_xml_entities(&out)
}

/// Collapse multiple whitespace characters into single spaces and trim.
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_space {
                result.push(' ');
                in_space = true;
            }
        } else {
            result.push(c);
            in_space = false;
        }
    }
    result.trim().to_string()
}

/// Extract attribute value by name from a tag's attribute string.
fn extract_attr(tag_header: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr_name);
    if let Some(start) = tag_header.find(&pattern) {
        let val_start = start + pattern.len();
        if let Some(val_end) = tag_header[val_start..].find('"') {
            return Some(tag_header[val_start..val_start + val_end].to_string());
        }
    }
    // Also try single quote
    let pattern_sq = format!("{}='", attr_name);
    if let Some(start) = tag_header.find(&pattern_sq) {
        let val_start = start + pattern_sq.len();
        if let Some(val_end) = tag_header[val_start..].find('\'') {
            return Some(tag_header[val_start..val_start + val_end].to_string());
        }
    }
    None
}

/// Parse Apple TTML XML into normalized `Lyrics`.
pub fn parse_ttml_lyrics(ttml: &str) -> Lyrics {
    if ttml.trim().is_empty() {
        return Lyrics::empty();
    }

    let mut lines = Vec::new();
    let mut synced = false;

    let mut rest = ttml;
    while let Some(p_start) = rest.find("<p") {
        rest = &rest[p_start..];
        let tag_close = match rest.find('>') {
            Some(idx) => idx,
            None => break,
        };

        let tag_header = &rest[2..tag_close];
        let is_self_closing = tag_header.trim_end().ends_with('/');

        let line_start_ms = extract_attr(tag_header, "begin").and_then(|s| parse_ttml_time(&s));
        let line_end_ms = extract_attr(tag_header, "end").and_then(|s| parse_ttml_time(&s));

        if is_self_closing {
            rest = &rest[tag_close + 1..];
            continue;
        }

        let content_start = tag_close + 1;
        let p_end = match rest[content_start..].find("</p>") {
            Some(idx) => content_start + idx,
            None => break,
        };

        let inner = &rest[content_start..p_end];
        rest = &rest[p_end + 4..];

        // Check for syllable spans: <span ...>...</span>
        let mut syllables = Vec::new();
        let mut span_rest = inner;
        while let Some(s_start) = span_rest.find("<span") {
            span_rest = &span_rest[s_start..];
            let s_tag_close = match span_rest.find('>') {
                Some(idx) => idx,
                None => break,
            };
            let s_header = &span_rest[5..s_tag_close];
            let s_content_start = s_tag_close + 1;
            let s_end = match span_rest[s_content_start..].find("</span>") {
                Some(idx) => s_content_start + idx,
                None => break,
            };
            let s_inner = &span_rest[s_content_start..s_end];
            span_rest = &span_rest[s_end + 7..];

            let s_begin = extract_attr(s_header, "begin").and_then(|s| parse_ttml_time(&s));
            let s_end_ms = extract_attr(s_header, "end").and_then(|s| parse_ttml_time(&s));
            let s_text = strip_tags_and_decode(s_inner);

            if !s_text.is_empty() {
                syllables.push(LyricSyllable::new(s_text, s_begin, s_end_ms));
            }
        }

        let full_text = if !syllables.is_empty() {
            syllables
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>()
                .trim()
                .to_string()
        } else {
            collapse_whitespace(&strip_tags_and_decode(inner))
        };
        if full_text.is_empty() {
            continue;
        }

        if line_start_ms.is_some() {
            synced = true;
        }

        let mut line = LyricLine::new(full_text);
        if let (Some(start), Some(end)) = (line_start_ms, line_end_ms) {
            line = line.with_timing(start, end);
        } else if let Some(start) = line_start_ms {
            line.start_ms = Some(start);
        }

        if !syllables.is_empty() {
            line = line.with_syllables(syllables);
        }

        lines.push(line);
    }

    Lyrics::new(lines, synced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ttml_time() {
        assert_eq!(parse_ttml_time("0.810"), Some(810));
        assert_eq!(parse_ttml_time("4.220"), Some(4220));
        assert_eq!(parse_ttml_time("00:15.20"), Some(15200));
        assert_eq!(parse_ttml_time("4:14.900"), Some(254900));
        assert_eq!(parse_ttml_time("01:02:03.450"), Some(3723450));
        assert_eq!(parse_ttml_time(""), None);
        assert_eq!(parse_ttml_time("invalid"), None);
    }

    #[test]
    fn test_parse_line_synced_lyrics() {
        let ttml = r#"
        <tt xmlns="http://www.w3.org/ns/ttml" xmlns:itunes="http://music.apple.com/lyric-ttml-internal" itunes:timing="Line" xml:lang="en">
            <head>
                <metadata>
                    <iTunesMetadata leadingSilence="0.160">
                        <songwriters><songwriter>Sai Abhyankkar</songwriter></songwriters>
                    </iTunesMetadata>
                </metadata>
            </head>
            <body dur="4:14.900">
                <div begin="0.810" end="2:06.790">
                    <p begin="0.810" end="4.220">First Line of Song</p>
                    <p begin="4.220" end="7.770">Second Line &amp; More</p>
                    <p begin="7.770" end="11.410">Third Line</p>
                </div>
            </body>
        </tt>
        "#;

        let lyrics = parse_ttml_lyrics(ttml);
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.lines[0].text, "First Line of Song");
        assert_eq!(lyrics.lines[0].start_ms, Some(810));
        assert_eq!(lyrics.lines[0].end_ms, Some(4220));

        assert_eq!(lyrics.lines[1].text, "Second Line & More");
        assert_eq!(lyrics.lines[1].start_ms, Some(4220));
        assert_eq!(lyrics.lines[1].end_ms, Some(7770));
    }

    #[test]
    fn test_parse_syllable_synced_lyrics() {
        let ttml = r#"
        <tt itunes:timing="Word">
            <body>
                <div>
                    <p begin="00:15.20" end="00:19.45">
                        <span begin="00:15.20" end="00:15.80">Karma </span>
                        <span begin="00:15.80" end="00:16.40">police</span>
                    </p>
                </div>
            </body>
        </tt>
        "#;

        let lyrics = parse_ttml_lyrics(ttml);
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines.len(), 1);
        let line = &lyrics.lines[0];
        assert_eq!(line.text, "Karma police");
        assert_eq!(line.start_ms, Some(15200));
        assert_eq!(line.end_ms, Some(19450));

        let syllables = line.syllables.as_ref().unwrap();
        assert_eq!(syllables.len(), 2);
        assert_eq!(syllables[0].text, "Karma ");
        assert_eq!(syllables[0].start_ms, Some(15200));
        assert_eq!(syllables[0].end_ms, Some(15800));
        assert_eq!(syllables[1].text, "police");
        assert_eq!(syllables[1].start_ms, Some(15800));
        assert_eq!(syllables[1].end_ms, Some(16400));
    }

    #[test]
    fn test_parse_unsynced_lyrics() {
        let ttml = r#"
        <tt>
            <body>
                <div>
                    <p>Unsynced first line</p>
                    <p>Unsynced second line</p>
                </div>
            </body>
        </tt>
        "#;

        let lyrics = parse_ttml_lyrics(ttml);
        assert!(!lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].text, "Unsynced first line");
        assert_eq!(lyrics.lines[0].start_ms, None);
        assert_eq!(lyrics.lines[1].text, "Unsynced second line");
        assert_eq!(lyrics.lines[1].start_ms, None);
    }

    #[test]
    fn test_empty_or_malformed_lyrics() {
        assert!(parse_ttml_lyrics("").is_empty());
        assert!(parse_ttml_lyrics("   ").is_empty());
        assert!(parse_ttml_lyrics("<tt></tt>").is_empty());
        assert!(parse_ttml_lyrics("<malformed>xml").is_empty());
    }
}
