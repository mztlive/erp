//! 权限生成物采用前端约定的 80 列中文描述格式。

use crate::escape_ts;

/// 输出权限描述，避免重新构建撤销生成物格式。
/// # 参数
/// 输出缓冲、属性缩进和原始描述。
/// # 返回
/// 将格式化后的属性追加到缓冲。
/// # 错误
/// 纯字符串生成，不执行外部操作。
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
