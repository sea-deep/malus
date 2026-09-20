//! TTML lyrics parsing for Apple Music using quick-xml.

use malus_model::{LyricLine, LyricSyllable, Lyrics};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

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

/// Parse Apple TTML XML into normalized `Lyrics` using quick-xml.
pub fn parse_ttml_lyrics(ttml: &str) -> Lyrics {
    if ttml.trim().is_empty() {
        return Lyrics::empty();
    }

    let mut reader = Reader::from_str(ttml);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut lines = Vec::new();
    let mut synced = false;

    let mut in_p = false;
    let mut p_start_ms: Option<u64> = None;
    let mut p_end_ms: Option<u64> = None;
    let mut p_agent: Option<String> = None;
    let mut p_text = String::new();
    let mut p_syllables: Vec<LyricSyllable> = Vec::new();

    let mut in_span = false;
    let mut span_start_ms: Option<u64> = None;
    let mut span_end_ms: Option<u64> = None;
    let mut span_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local_name = e.local_name();
                if local_name.as_ref() == "p" {
                    in_p = true;
                    p_start_ms = None;
                    p_end_ms = None;
                    p_agent = None;
                    p_text.clear();
                    p_syllables.clear();

                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        if key == "begin" {
                            p_start_ms = parse_ttml_time(attr.value.as_ref());
                        } else if key == "end" {
                            p_end_ms = parse_ttml_time(attr.value.as_ref());
                        } else if key == "ttm:agent" {
                            p_agent = Some(attr.value.as_ref().to_string());
                        }
                    }
                } else if in_p && local_name.as_ref() == "span" {
                    in_span = true;
                    span_start_ms = None;
                    span_end_ms = None;
                    span_text.clear();

                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == "begin" {
                            span_start_ms = parse_ttml_time(attr.value.as_ref());
                        } else if attr.key.as_ref() == "end" {
                            span_end_ms = parse_ttml_time(attr.value.as_ref());
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.as_ref();
                if in_span {
                    span_text.push_str(text);
                } else if in_p {
                    p_text.push_str(text);
                }
            }
            Ok(Event::CData(ref e)) => {
                let text = e.as_ref();
                if in_span {
                    span_text.push_str(text);
                } else if in_p {
                    p_text.push_str(text);
                }
            }
            Ok(Event::GeneralRef(ref e)) => {
                let text = if let Some(predefined) =
                    quick_xml::escape::resolve_predefined_entity(e.as_ref())
                {
                    predefined.to_string()
                } else if let Ok(Some(ch)) = e.resolve_char_ref() {
                    ch.to_string()
                } else {
                    format!("&{};", e.as_ref())
                };

                if in_span {
                    span_text.push_str(&text);
                } else if in_p {
                    p_text.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = e.local_name();
                if in_span && local_name.as_ref() == "span" {
                    if !span_text.is_empty() {
                        p_syllables.push(LyricSyllable::new(
                            span_text.clone(),
                            span_start_ms,
                            span_end_ms,
                        ));
                    }
                    in_span = false;
                } else if in_p && local_name.as_ref() == "p" {
                    let full_text = if !p_syllables.is_empty() {
                        let mut joined = String::new();
                        for syl in &p_syllables {
                            if !joined.is_empty()
                                && !joined.ends_with(char::is_whitespace)
                                && !syl.text.starts_with(char::is_whitespace)
                            {
                                joined.push(' ');
                            }
                            joined.push_str(&syl.text);
                        }
                        joined.trim().to_string()
                    } else {
                        collapse_whitespace(&p_text)
                    };

                    if !full_text.is_empty() {
                        if p_start_ms.is_some() {
                            synced = true;
                        }
                        let mut line = LyricLine::new(full_text);
                        if let (Some(s), Some(e)) = (p_start_ms, p_end_ms) {
                            line = line.with_timing(s, e);
                        } else if let Some(s) = p_start_ms {
                            line.start_ms = Some(s);
                        }
                        if !p_syllables.is_empty() {
                            line = line.with_syllables(p_syllables.clone());
                        }
                        line.agent = p_agent.clone();
                        lines.push(line);
                    }
                    in_p = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
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
    fn test_syllable_spans_without_trailing_whitespace() {
        let ttml = r#"
        <tt itunes:timing="Word">
            <body>
                <div>
                    <p begin="00:15.20" end="00:19.45">
                        <span begin="00:15.20" end="00:15.80">Chhali</span>
                        <span begin="00:15.80" end="00:16.40">karde</span>
                        <span begin="00:16.40" end="00:17.00">dil</span>
                        <span begin="00:17.00" end="00:17.60">mera</span>
                    </p>
                </div>
            </body>
        </tt>
        "#;

        let lyrics = parse_ttml_lyrics(ttml);
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "Chhali karde dil mera");
    }

    #[test]
    fn test_parse_unsynced_lyrics() {
        let ttml = r#"
        <tt>
            <body>
                <div>
                    <p>Unsynced line one</p>
                    <p>Unsynced line two</p>
                </div>
            </body>
        </tt>
        "#;

        let lyrics = parse_ttml_lyrics(ttml);
        assert!(!lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].text, "Unsynced line one");
        assert_eq!(lyrics.lines[0].start_ms, None);
        assert_eq!(lyrics.lines[1].text, "Unsynced line two");
        assert_eq!(lyrics.lines[1].start_ms, None);
    }

    #[test]
    fn test_parse_xml_entities() {
        let ttml = r#"
        <tt>
            <body>
                <div>
                    <p begin="1.0" end="2.0">Tom &amp; Jerry &lt;&quot;Special&quot;&gt; &apos;Edition&apos;</p>
                </div>
            </body>
        </tt>
        "#;
        let lyrics = parse_ttml_lyrics(ttml);
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "Tom & Jerry <\"Special\"> 'Edition'");
    }

    #[test]
    fn test_parse_multilingual_unicode() {
        let ttml = r#"
        <tt>
            <body>
                <div>
                    <p begin="0.810" end="4.220">یا عَلی، یا عَلی، झूम</p>
                    <p begin="4.220" end="7.770">ஹையா, ஏ ஹையா-ஹையா</p>
                    <p begin="7.770" end="10.000">初音ミクの消失 🎵</p>
                </div>
            </body>
        </tt>
        "#;
        let lyrics = parse_ttml_lyrics(ttml);
        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.lines[0].text, "یا عَلی، یا عَلی، झूम");
        assert_eq!(lyrics.lines[1].text, "ஹையா, ஏ ஹையா-ஹையா");
        assert_eq!(lyrics.lines[2].text, "初音ミクの消失 🎵");
    }

    #[test]
    fn test_empty_or_malformed_lyrics() {
        assert_eq!(parse_ttml_lyrics("").lines.len(), 0);
        assert_eq!(parse_ttml_lyrics("   ").lines.len(), 0);
        assert_eq!(parse_ttml_lyrics("<tt><body/></tt>").lines.len(), 0);
        assert_eq!(parse_ttml_lyrics("not xml").lines.len(), 0);
        // Truncated / malformed XML should recover cleanly
        let broken = "<tt><body><div><p begin=\"1.0\" end=\"2.0\">Recovered line";
        let lyrics = parse_ttml_lyrics(broken);
        assert_eq!(lyrics.lines.len(), 0);
    }
}
