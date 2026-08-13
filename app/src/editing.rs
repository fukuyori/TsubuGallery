//! コード編集の操作 (設計書 §25 の Editor)。
//!
//! 文字列とカーソル位置だけを扱う純粋な処理として書いてある。UI から切り離して
//! あるので、egui を動かさずに端の条件を確かめられる。
//!
//! 位置は **文字単位**。egui のカーソルと同じ数え方で、日本語のコメントが
//! 混ざっていてもずれない。

/// 字下げ 1 段。整形 ([`tsubu_processing_lite::format`]) と揃える。
pub const INDENT: &str = "  ";

/// 文字単位の範囲。
pub type CharRange = std::ops::Range<usize>;

/// 編集の結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    /// 編集後のカーソル。範囲が空ならキャレット。
    pub selection: CharRange,
}

/// 改行を入れ、前の行と同じところまで字下げする。
///
/// 行が `{` で終わっていれば 1 段深くする。カーソルの直後が `}` なら、閉じ括弧を
/// さらに次の行へ送って間を空ける。
pub fn newline_with_indent(text: &str, selection: CharRange) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let selection = clamp(&chars, selection);

    let (line_start, _) = line_bounds(&chars, selection.start);
    let indent: String = chars[line_start..selection.start]
        .iter()
        .take_while(|c| **c == ' ' || **c == '\t')
        .collect();

    let before = chars[line_start..selection.start].iter().collect::<String>();
    let deeper = before.trim_end().ends_with('{');

    let mut inserted = String::from("\n");
    inserted.push_str(&indent);
    if deeper {
        inserted.push_str(INDENT);
    }
    let cursor = selection.start + inserted.chars().count();

    // `{|}` の形なら、閉じ括弧を次の行へ送る。
    let closes = chars.get(selection.end) == Some(&'}');
    if deeper && closes {
        inserted.push('\n');
        inserted.push_str(&indent);
    }

    Edit { text: splice(&chars, &selection, &inserted), selection: cursor..cursor }
}

/// 選択している行をまとめて字下げする。選択が無ければその場に空白を入れる。
pub fn indent(text: &str, selection: CharRange) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let selection = clamp(&chars, selection);

    if selection.is_empty() {
        let cursor = selection.start + INDENT.chars().count();
        return Edit { text: splice(&chars, &selection, INDENT), selection: cursor..cursor };
    }

    edit_lines(&chars, selection, |line| {
        let mut out = String::from(INDENT);
        out.push_str(line);
        out
    })
}

/// 選択している行の字下げを 1 段戻す。
pub fn outdent(text: &str, selection: CharRange) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let selection = clamp(&chars, selection);

    edit_lines(&chars, selection, |line| {
        if let Some(rest) = line.strip_prefix(INDENT) {
            rest.to_string()
        } else if let Some(rest) = line.strip_prefix('\t') {
            rest.to_string()
        } else {
            line.trim_start_matches(' ').to_string()
        }
    })
}

/// 選択している行の行コメントを付け外しする。
///
/// すべての行が既にコメントなら外し、そうでなければ付ける。
pub fn toggle_comment(text: &str, selection: CharRange) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let selection = clamp(&chars, selection);

    let lines = touched_lines(&chars, &selection);
    let all_commented = lines
        .iter()
        .map(|(start, end)| chars[*start..*end].iter().collect::<String>())
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with("//"));

    // 付けるときは、いちばん浅い行に合わせて桁を揃える。
    let column = lines
        .iter()
        .map(|(start, end)| chars[*start..*end].iter().collect::<String>())
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    edit_lines(&chars, selection, |line| {
        if line.trim().is_empty() {
            return line.to_string();
        }
        if all_commented {
            let at = line.find("//").expect("すべてコメントだと確かめてある");
            let mut out = line[..at].to_string();
            out.push_str(line[at + 2..].strip_prefix(' ').unwrap_or(&line[at + 2..]));
            out
        } else {
            let at = column.min(line.len());
            format!("{}// {}", &line[..at], &line[at..])
        }
    })
}

/// 選択している行を真下へ複製する。
pub fn duplicate_lines(text: &str, selection: CharRange) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let selection = clamp(&chars, selection);

    let lines = touched_lines(&chars, &selection);
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
        return Edit { text: text.to_string(), selection };
    };
    let block: String = chars[first.0..last.1].iter().collect();

    let inserted = format!("\n{block}");
    let shift = inserted.chars().count();
    Edit {
        text: splice(&chars, &(last.1..last.1), &inserted),
        // 複製したほうへカーソルを移す。続けて直せる。
        selection: (selection.start + shift)..(selection.end + shift),
    }
}

/// 選択している行を 1 行ぶん上下へ動かす。
pub fn move_lines(text: &str, selection: CharRange, delta: i32) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let selection = clamp(&chars, selection);
    let unchanged = Edit { text: text.to_string(), selection: selection.clone() };

    let lines = touched_lines(&chars, &selection);
    let (Some(first), Some(last)) = (lines.first().copied(), lines.last().copied()) else {
        return unchanged;
    };

    let block: String = chars[first.0..last.1].iter().collect();

    if delta < 0 {
        if first.0 == 0 {
            return unchanged;
        }
        let (above_start, above_end) = line_bounds(&chars, first.0 - 1);
        let above: String = chars[above_start..above_end].iter().collect();

        let replaced = format!("{block}\n{above}");
        let shift = first.0 - above_start;
        return Edit {
            text: splice(&chars, &(above_start..last.1), &replaced),
            selection: (selection.start - shift)..(selection.end - shift),
        };
    }

    if last.1 >= chars.len() {
        return unchanged;
    }
    let (below_start, below_end) = line_bounds(&chars, last.1 + 1);
    let below: String = chars[below_start..below_end].iter().collect();

    let replaced = format!("{below}\n{block}");
    let shift = below_end - last.1;
    Edit {
        text: splice(&chars, &(first.0..below_end), &replaced),
        selection: (selection.start + shift)..(selection.end + shift),
    }
}

/// 1 始まりの行の先頭を指すカーソル位置。エラー行へ飛ぶのに使う。
pub fn start_of_line(text: &str, line: u32) -> usize {
    let mut at = 0usize;
    let mut current = 1u32;
    for c in text.chars() {
        if current == line {
            return at;
        }
        at += 1;
        if c == '\n' {
            current += 1;
        }
    }
    at.min(text.chars().count())
}

// ---------------------------------------------------------------------------

fn clamp(chars: &[char], selection: CharRange) -> CharRange {
    let start = selection.start.min(chars.len());
    let end = selection.end.min(chars.len());
    start.min(end)..start.max(end)
}

/// `at` を含む行の範囲。末尾の改行は含めない。
fn line_bounds(chars: &[char], at: usize) -> (usize, usize) {
    let at = at.min(chars.len());
    let start = chars[..at].iter().rposition(|c| *c == '\n').map_or(0, |i| i + 1);
    let end = chars[at..].iter().position(|c| *c == '\n').map_or(chars.len(), |i| at + i);
    (start, end)
}

/// 選択に触れている行すべて。
fn touched_lines(chars: &[char], selection: &CharRange) -> Vec<(usize, usize)> {
    let (first_start, _) = line_bounds(chars, selection.start);
    // 選択が行頭で終わっているときは、その行を含めない。
    let last_at = if selection.end > selection.start
        && chars.get(selection.end.wrapping_sub(1)) == Some(&'\n')
    {
        selection.end - 1
    } else {
        selection.end
    };
    let (_, last_end) = line_bounds(chars, last_at);

    let mut lines = Vec::new();
    let mut at = first_start;
    loop {
        let (start, end) = line_bounds(chars, at);
        lines.push((start, end));
        if end >= last_end {
            break;
        }
        at = end + 1;
    }
    lines
}

/// 選択に触れている行を 1 行ずつ書き換える。選択は書き換え後の全体を覆う。
fn edit_lines(
    chars: &[char],
    selection: CharRange,
    mut transform: impl FnMut(&str) -> String,
) -> Edit {
    let lines = touched_lines(chars, &selection);
    let (Some(first), Some(last)) = (lines.first().copied(), lines.last().copied()) else {
        return Edit { text: chars.iter().collect(), selection };
    };

    let replaced: Vec<String> = lines
        .iter()
        .map(|(start, end)| transform(&chars[*start..*end].iter().collect::<String>()))
        .collect();
    let joined = replaced.join("\n");

    let text = splice(chars, &(first.0..last.1), &joined);
    let end = first.0 + joined.chars().count();
    Edit { text, selection: first.0..end }
}

/// 範囲を置き換えた文字列を作る。
fn splice(chars: &[char], range: &CharRange, replacement: &str) -> String {
    let mut out: String = chars[..range.start].iter().collect();
    out.push_str(replacement);
    out.extend(&chars[range.end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `|` をカーソル、`[` `]` を選択範囲として書ける記法。
    fn parse(marked: &str) -> (String, CharRange) {
        if let (Some(open), Some(close)) = (marked.find('['), marked.find(']')) {
            let text: String = marked.chars().filter(|c| *c != '[' && *c != ']').collect();
            let start = marked[..open].chars().count();
            let end = marked[..close].chars().count() - 1;
            return (text, start..end);
        }
        let at = marked.find('|').expect("カーソルの印がない");
        let text: String = marked.chars().filter(|c| *c != '|').collect();
        let cursor = marked[..at].chars().count();
        (text, cursor..cursor)
    }

    fn show(edit: &Edit) -> String {
        let chars: Vec<char> = edit.text.chars().collect();
        let mut out: String = chars[..edit.selection.start].iter().collect();
        if edit.selection.is_empty() {
            out.push('|');
        } else {
            out.push('[');
            out.extend(&chars[edit.selection.clone()]);
            out.push(']');
        }
        out.extend(&chars[edit.selection.end..]);
        out
    }

    fn apply(marked: &str, f: impl Fn(&str, CharRange) -> Edit) -> String {
        let (text, selection) = parse(marked);
        show(&f(&text, selection))
    }

    // ---- 改行と字下げ ---------------------------------------------------

    #[test]
    fn newline_keeps_the_current_indentation() {
        assert_eq!(apply("  point(1);|", newline_with_indent), "  point(1);\n  |");
    }

    #[test]
    fn newline_after_an_open_brace_goes_one_level_deeper() {
        assert_eq!(apply("void draw() {|", newline_with_indent), "void draw() {\n  |");
    }

    #[test]
    fn newline_between_braces_puts_the_closing_one_on_its_own_line() {
        assert_eq!(apply("void draw() {|}", newline_with_indent), "void draw() {\n  |\n}");
    }

    #[test]
    fn newline_replaces_the_selection() {
        assert_eq!(apply("  a[bc]d", newline_with_indent), "  a\n  |d");
    }

    // ---- 字下げ ---------------------------------------------------------

    #[test]
    fn tab_inserts_spaces_when_nothing_is_selected() {
        assert_eq!(apply("a|b", indent), "a  |b");
    }

    #[test]
    fn tab_indents_every_touched_line() {
        assert_eq!(apply("[a\nb]\nc", indent), "[  a\n  b]\nc");
    }

    #[test]
    fn shift_tab_removes_one_level() {
        assert_eq!(apply("[    a\n  b]", outdent), "[  a\nb]");
    }

    #[test]
    fn shift_tab_on_an_unindented_line_does_nothing() {
        assert_eq!(apply("[a]", outdent), "[a]");
    }

    #[test]
    fn a_selection_ending_at_a_line_start_does_not_touch_the_next_line() {
        // 選択が改行で終わっていても、次の行は動かさない。
        let (text, selection) = parse("[a\n]b");
        assert_eq!(indent(&text, selection).text, "  a\nb");
    }

    // ---- コメント -------------------------------------------------------

    #[test]
    fn comment_is_added_at_the_shallowest_indentation() {
        assert_eq!(apply("[  a\n    b]", toggle_comment), "[  // a\n  //   b]");
    }

    #[test]
    fn comment_is_removed_when_every_line_has_one() {
        assert_eq!(apply("[// a\n// b]", toggle_comment), "[a\nb]");
    }

    #[test]
    fn a_partly_commented_selection_gets_commented() {
        assert_eq!(apply("[// a\nb]", toggle_comment), "[// // a\n// b]");
    }

    #[test]
    fn blank_lines_are_left_alone() {
        assert_eq!(apply("[a\n\nb]", toggle_comment), "[// a\n\n// b]");
    }

    // ---- 行の複製と移動 -------------------------------------------------

    #[test]
    fn duplicate_puts_a_copy_below_and_follows_it() {
        assert_eq!(apply("a|b\nc", duplicate_lines), "ab\na|b\nc");
    }

    #[test]
    fn move_up_swaps_with_the_line_above() {
        assert_eq!(apply("a\nb|\nc", |t, s| move_lines(t, s, -1)), "b|\na\nc");
    }

    #[test]
    fn move_down_swaps_with_the_line_below() {
        assert_eq!(apply("a|\nb\nc", |t, s| move_lines(t, s, 1)), "b\na|\nc");
    }

    #[test]
    fn moving_past_the_edges_does_nothing() {
        assert_eq!(apply("a|\nb", |t, s| move_lines(t, s, -1)), "a|\nb");
        assert_eq!(apply("a\nb|", |t, s| move_lines(t, s, 1)), "a\nb|");
    }

    #[test]
    fn moving_a_block_keeps_it_together() {
        assert_eq!(apply("a\n[b\nc]\nd", |t, s| move_lines(t, s, -1)), "[b\nc]\na\nd");
    }

    // ---- 端の条件 -------------------------------------------------------

    #[test]
    fn multibyte_text_does_not_shift_the_cursor() {
        let (text, selection) = parse("// つぶやき|");
        let edit = newline_with_indent(&text, selection);
        assert_eq!(edit.text, "// つぶやき\n");
        assert_eq!(edit.selection.start, "// つぶやき\n".chars().count());
    }

    #[test]
    fn an_empty_buffer_is_safe() {
        for f in [
            newline_with_indent as fn(&str, CharRange) -> Edit,
            indent,
            outdent,
            toggle_comment,
            duplicate_lines,
        ] {
            let edit = f("", 0..0);
            assert!(edit.selection.end <= edit.text.chars().count());
        }
    }

    #[test]
    fn an_out_of_range_selection_is_clamped() {
        let edit = indent("ab", 99..200);
        assert!(edit.selection.end <= edit.text.chars().count());
    }

    #[test]
    fn line_starts_are_one_based() {
        let text = "a\nbb\nccc";
        assert_eq!(start_of_line(text, 1), 0);
        assert_eq!(start_of_line(text, 2), 2);
        assert_eq!(start_of_line(text, 3), 5);
        assert_eq!(start_of_line(text, 99), text.chars().count());
    }

    #[test]
    fn line_starts_count_characters_not_bytes() {
        let text = "// つぶやき\nint x;";
        assert_eq!(start_of_line(text, 2), "// つぶやき\n".chars().count());
    }
}
