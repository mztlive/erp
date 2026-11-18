//! 权限生成物采用前端约定的 80 列中文描述格式。

use crate::escape_ts;

// 输出生成目录的描述属性时沿用既有缩进和排版，避免内容不变的权限被重新生成成无关格式差异。
pub(super) fn push_description(content: &mut String, indentation: usize, value: &str) {
    let escaped = escape_ts(value);
    let prefix = " ".repeat(indentation);
    let line = format!("{prefix}description: \"{escaped}\",\n");
    let width = line
        .trim_end()
        .chars()
        .map(|c| match c {
            '\u{2e80}'..='\u{a4cf}' | '\u{ac00}'..='\u{d7a3}' | '\u{ff01}'..='\u{ff60}' => 2,
            _ => 1,
        })
        .sum::<usize>();
    if width > 80 {
        content.push_str(&format!("{prefix}description:\n{prefix}    \"{escaped}\",\n"));
    } else {
        content.push_str(&line);
    }
}
