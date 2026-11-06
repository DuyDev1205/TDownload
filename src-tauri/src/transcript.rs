//! Phụ đề "chỉ chữ": đổi file .srt thành .txt chỉ có lời thoại, gộp thành một đoạn văn.
//! Phụ đề tự động của YouTube "cuộn" (mỗi khung lặp lại dòng của khung trước), nên phần trùng với cuối đoạn đã có
//! được bỏ đi. Chỉ bỏ khi trùng cả dòng hoặc từ 3 từ trở lên, để không xoá nhầm từ người nói lặp thật ("no no").

use std::path::Path;

/// Chữ Hán, Hiragana, Katakana: viết liền không cách nên không chèn dấu cách khi ghép.
fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

/// Bỏ thẻ định dạng: <i>, <font ...>, {\an8}...
fn strip_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut close = None;
    for c in line.chars() {
        match (close, c) {
            (None, '<') => close = Some('>'),
            (None, '{') => close = Some('}'),
            (Some(end), _) if c == end => close = None,
            (Some(_), _) => {}
            (None, _) => out.push(c),
        }
    }
    out
}

fn decode_entities(line: &str) -> String {
    line.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Cả dòng là ghi chú âm thanh, ví dụ "(Laughter)", "[Music]", "[Âm nhạc]".
fn is_annotation(line: &str) -> bool {
    if line.chars().count() < 2 {
        return false;
    }
    let (Some(first), Some(last)) = (line.chars().next(), line.chars().last()) else { return false };
    let inner = &line[first.len_utf8()..line.len() - last.len_utf8()];
    matches!(first, '[' | '(') && matches!(last, ']' | ')') && !inner.contains([']', ')'])
}

/// Lời thoại trong file .srt thành một đoạn văn: bỏ số thứ tự, mốc thời gian, thẻ định dạng, ghi chú âm thanh và
/// phần lặp của phụ đề cuộn.
pub fn srt_to_text(srt: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for raw in srt.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.is_empty() || line.chars().all(|c| c.is_ascii_digit()) || line.contains("-->") {
            continue;
        }
        let line: String = decode_entities(&strip_tags(line)).chars().filter(|c| !matches!(c, '♪' | '♫')).collect();
        let line = line.trim();
        if line.is_empty() || is_annotation(line) {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let max = tokens.len().min(words.len()).min(30);
        let overlap = (1..=max)
            .rev()
            .find(|&n| words[words.len() - n..].iter().map(String::as_str).eq(tokens[..n].iter().copied()))
            .unwrap_or(0);
        let skip = if overlap == tokens.len() || overlap >= 3 { overlap } else { 0 };
        words.extend(tokens[skip..].iter().map(|s| s.to_string()));
    }

    let mut out = String::new();
    for w in &words {
        let joins_cjk = out.chars().last().is_some_and(is_cjk) && w.chars().next().is_some_and(is_cjk);
        if !out.is_empty() && !joins_cjk {
            out.push(' ');
        }
        out.push_str(w);
    }
    match out.chars().next() {
        Some(first) => format!("{}{}", first.to_uppercase(), &out[first.len_utf8()..]),
        None => out,
    }
}

/// Đổi mọi file phụ đề .srt đi kèm `media` (cùng tên gốc, ví dụ "Video [id].en.srt") thành .txt chỉ có chữ, rồi xoá
/// file .srt. Trả về số file đã đổi.
pub fn convert_beside(media: &Path) -> usize {
    let (Some(dir), Some(stem)) = (media.parent(), media.file_stem().and_then(|s| s.to_str())) else { return 0 };
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let prefix = format!("{stem}.");
    let mut count = 0;
    for path in entries.flatten().map(|e| e.path()) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with(&prefix) || !name.to_lowercase().ends_with(".srt") {
            continue;
        }
        let Ok(srt) = std::fs::read_to_string(&path) else { continue };
        let text = srt_to_text(&srt);
        if text.is_empty() {
            continue;
        }
        if std::fs::write(path.with_extension("txt"), format!("{text}\n")).is_ok() {
            let _ = std::fs::remove_file(&path);
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::srt_to_text;

    #[test]
    fn rolling_auto_captions_become_one_paragraph() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n \nso in\n\n2\n00:00:02,000 --> 00:00:02,010\nso in\n \n\n\
                   3\n00:00:02,010 --> 00:00:04,000\nso in\ncollege I was a government major which\n\n\
                   4\n00:00:04,000 --> 00:00:04,010\ncollege I was a government major which\n \n";
        assert_eq!(srt_to_text(srt), "So in college I was a government major which");
    }

    #[test]
    fn drops_tags_annotations_and_keeps_real_repeats() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n<i>No</i> no,\n\n2\n00:00:02,000 --> 00:00:03,000\n(Laughter)\n\n\
                   3\n00:00:03,000 --> 00:00:04,000\nno &amp; yes ♪\n";
        assert_eq!(srt_to_text(srt), "No no, no & yes");
    }

    #[test]
    fn cjk_is_joined_without_spaces() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n你好\n\n2\n00:00:02,000 --> 00:00:03,000\n世界\n";
        assert_eq!(srt_to_text(srt), "你好世界");
    }
}
