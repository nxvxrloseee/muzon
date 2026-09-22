use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct LrcWord {
    pub time_secs: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct LrcLine {
    pub time_secs: f64,
    pub text: String,
    pub words: Option<Vec<LrcWord>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default, Type)]
pub struct Lyrics {
    pub lines: Vec<LrcLine>,
}

/// Parses standard line-level LRC (`[mm:ss.xx]text`) and enhanced word-level LRC
/// (`[mm:ss.xx]<mm:ss.xx>word <mm:ss.xx>word`), skipping metadata tags (`[ar:...]`)
/// and blank lines. A line may carry multiple leading timestamps (a repeated chorus).
pub fn parse(input: &str) -> Lyrics {
    let mut lines = Vec::new();

    for raw_line in input.lines() {
        let raw_line = raw_line.trim();
        if raw_line.is_empty() {
            continue;
        }

        let mut rest = raw_line;
        let mut timestamps = Vec::new();
        while let Some((t, remainder)) = parse_leading_tag(rest) {
            timestamps.push(t);
            rest = remainder;
        }
        if timestamps.is_empty() {
            continue;
        }

        let (plain_text, words) = parse_words(rest);

        for t in timestamps {
            lines.push(LrcLine {
                time_secs: t,
                text: plain_text.clone(),
                words: if words.is_empty() {
                    None
                } else {
                    Some(words.clone())
                },
            });
        }
    }

    lines.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap());
    Lyrics { lines }
}

fn parse_leading_tag(s: &str) -> Option<(f64, &str)> {
    let s = s.strip_prefix('[')?;
    let end = s.find(']')?;
    let tag = &s[..end];
    let rest = &s[end + 1..];
    let time = parse_time(tag)?;
    Some((time, rest))
}

fn parse_time(tag: &str) -> Option<f64> {
    let parts: Vec<&str> = tag.splitn(2, ':').collect();
    if parts.len() != 2 {
        return None;
    }
    let minutes: f64 = parts[0].parse().ok()?;
    let seconds: f64 = parts[1].replace(':', ".").parse().ok()?;
    Some(minutes * 60.0 + seconds)
}

fn parse_words(s: &str) -> (String, Vec<LrcWord>) {
    let mut words = Vec::new();
    let mut plain = String::new();
    let mut rest = s;

    while let Some(start) = rest.find('<') {
        plain.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            plain.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = &after[..end];
        let remainder = &after[end + 1..];

        if let Some(time) = parse_time(tag) {
            let next_tag = remainder.find('<').unwrap_or(remainder.len());
            let word_text = remainder[..next_tag].to_string();
            plain.push_str(&word_text);
            words.push(LrcWord {
                time_secs: time,
                text: word_text,
            });
            rest = &remainder[next_tag..];
        } else {
            plain.push('<');
            plain.push_str(tag);
            plain.push('>');
            rest = remainder;
        }
    }
    plain.push_str(rest);

    (plain.trim().to_string(), words)
}

/// How far a candidate's duration may be from the track's before it is taken to
/// be a different recording. LRCLIB matches within a couple of seconds itself;
/// stricter than this rejects legitimate hits (encoders disagree about trailing
/// silence), looser starts attaching another song's timings.
const DURATION_TOLERANCE_SECS: f64 = 3.0;

/// The parts of a lyrics search result that decide whether it is the right one.
pub struct Match {
    pub has_synced: bool,
    pub duration_secs: Option<f64>,
}

/// Index of the candidate most likely to be this recording, if any.
///
/// Timed lyrics beat plain text outright: the whole lyrics view is built on
/// timings, and untimed text is only worth having as a last resort. Among
/// equals the closest duration wins. When the track's own duration is known,
/// anything outside the tolerance is dropped rather than guessed at - no lyrics
/// is a much better answer than confidently showing another song's.
pub fn pick_best_match(candidates: &[Match], duration_secs: Option<f64>) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let distance = match (duration_secs, candidate.duration_secs) {
                (Some(wanted), Some(found)) => {
                    let distance = (wanted - found).abs();
                    if distance > DURATION_TOLERANCE_SECS {
                        return None;
                    }
                    distance
                }
                // Unknown on either side: acceptable, but ranked behind anything
                // that could actually be compared.
                _ => f64::INFINITY,
            };
            Some((index, candidate.has_synced, distance))
        })
        .min_by(|a, b| b.1.cmp(&a.1).then(a.2.total_cmp(&b.2)))
        .map(|(index, _, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(has_synced: bool, duration_secs: Option<f64>) -> Match {
        Match {
            has_synced,
            duration_secs,
        }
    }

    #[test]
    fn timed_lyrics_win_over_plain_ones() {
        let candidates = [candidate(false, Some(200.0)), candidate(true, Some(200.0))];
        assert_eq!(pick_best_match(&candidates, Some(200.0)), Some(1));
    }

    #[test]
    fn among_equals_the_closest_duration_wins() {
        let candidates = [
            candidate(true, Some(202.0)),
            candidate(true, Some(200.5)),
            candidate(true, Some(198.0)),
        ];
        assert_eq!(pick_best_match(&candidates, Some(200.0)), Some(1));
    }

    #[test]
    fn a_recording_of_the_wrong_length_is_rejected_outright() {
        // A live version twice the length is not this song, however confident
        // the search was about the title.
        let candidates = [candidate(true, Some(400.0))];
        assert_eq!(pick_best_match(&candidates, Some(200.0)), None);
    }

    #[test]
    fn a_close_enough_duration_still_counts() {
        let candidates = [candidate(true, Some(202.5))];
        assert_eq!(pick_best_match(&candidates, Some(200.0)), Some(0));
    }

    #[test]
    fn plain_lyrics_of_the_right_length_beat_timed_ones_of_the_wrong_length() {
        let candidates = [candidate(true, Some(400.0)), candidate(false, Some(200.0))];
        assert_eq!(pick_best_match(&candidates, Some(200.0)), Some(1));
    }

    #[test]
    fn an_unknown_duration_falls_back_to_preferring_timed_lyrics() {
        let candidates = [candidate(false, Some(200.0)), candidate(true, None)];
        assert_eq!(pick_best_match(&candidates, None), Some(1));
    }

    #[test]
    fn nothing_to_choose_from_is_not_a_match() {
        assert_eq!(pick_best_match(&[], Some(200.0)), None);
    }


    #[test]
    fn parses_basic_line_level_lrc() {
        let lyrics = parse("[00:01.00]Hello world\n[00:05.50]Second line\n");
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].time_secs, 1.0);
        assert_eq!(lyrics.lines[0].text, "Hello world");
        assert_eq!(lyrics.lines[1].time_secs, 5.5);
        assert_eq!(lyrics.lines[1].text, "Second line");
    }

    #[test]
    fn skips_metadata_and_blank_lines() {
        let lyrics = parse("[ar:Artist]\n[ti:Title]\n\n[00:02.00]Real line\n");
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "Real line");
    }

    #[test]
    fn parses_word_level_enhanced_lrc() {
        let lyrics = parse("[00:10.00]<00:10.00>Hello <00:10.50>world\n");
        assert_eq!(lyrics.lines.len(), 1);
        let words = lyrics.lines[0].words.as_ref().expect("words present");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0], LrcWord { time_secs: 10.0, text: "Hello ".into() });
        assert_eq!(words[1], LrcWord { time_secs: 10.5, text: "world".into() });
        assert_eq!(lyrics.lines[0].text, "Hello world");
    }

    #[test]
    fn handles_multiple_timestamps_sharing_one_line() {
        let lyrics = parse("[00:01.00][00:30.00]Chorus line\n");
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].time_secs, 1.0);
        assert_eq!(lyrics.lines[1].time_secs, 30.0);
        assert_eq!(lyrics.lines[0].text, lyrics.lines[1].text);
    }

    #[test]
    fn sorts_lines_by_time_regardless_of_source_order() {
        let lyrics = parse("[00:05.00]Later\n[00:01.00]Earlier\n");
        assert_eq!(lyrics.lines[0].text, "Earlier");
        assert_eq!(lyrics.lines[1].text, "Later");
    }

    #[test]
    fn plain_line_without_timestamp_is_ignored() {
        let lyrics = parse("just some text with no timestamp\n[00:01.00]Real\n");
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "Real");
    }
}
