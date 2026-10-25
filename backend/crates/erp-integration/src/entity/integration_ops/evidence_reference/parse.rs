//! 证据引用段解析、集合编码与 512 字节上限。

use erp_core::{Error, Result};

/// 单条引用或集合编码的最大 UTF-8 字节数。
pub(super) const ENCODED_MAX_LEN: usize = 512;
/// 对象类型 / 证据类型代码最大长度。
const KIND_MAX_LEN: usize = 64;
/// 身份 ID 最大长度。
const ID_MAX_LEN: usize = 256;
/// 状态段最大长度。
const STATUS_MAX_LEN: usize = 64;
/// 会切断或伪造引用边界的字符。
const DELIMITERS: &[char] = &[':', '/', ';', '|', ',', '='];

/// 对集合成员排序去重后按固定分隔符连接。
///
/// # 参数
/// * `members` - 已编码的成员
/// * `separator` - `;` 或 `,`
/// * `reject_empty` - 空集合是否失败
/// * `length_message` - 超长或空集合错误
///
/// # 返回
/// 返回规范化编码。
///
/// # 错误
/// 空集合被拒绝或超过 512 字节时返回领域校验错误。
///
/// # 约束
/// 排序使用字典序，重复成员只保留一次。
pub(super) fn encode_set(
    members: impl IntoIterator<Item = String>,
    separator: char,
    reject_empty: bool,
    length_message: &str,
) -> Result<String> {
    let mut encoded = members.into_iter().collect::<Vec<_>>();
    encoded.sort();
    encoded.dedup();
    if reject_empty && encoded.is_empty() {
        return Err(Error::from(length_message));
    }
    let value = encoded.join(&separator.to_string());
    if value.len() > ENCODED_MAX_LEN {
        return Err(Error::from(length_message));
    }
    Ok(value)
}

/// 校验单条编码未超过 512 字节。
///
/// # 参数
/// * `value` - 已编码文本
///
/// # 返回
/// 长度合法时返回 `Ok(())`。
///
/// # 错误
/// 超过 512 字节时返回领域校验错误。
///
/// # 约束
/// 按 UTF-8 字节计数，与既有 `.len()` 门禁一致。
pub(super) fn ensure_encoded_len(value: &str) -> Result<()> {
    if value.len() > ENCODED_MAX_LEN {
        return Err(Error::from("终态证据引用为空或过长"));
    }
    Ok(())
}

/// 解析对象类型或证据类型代码。
///
/// # 参数
/// * `value` - 类型段
/// * `message` - 失败消息
///
/// # 返回
/// 返回已校验的类型代码。
///
/// # 错误
/// 空、超长、非标识或含分隔符时返回领域校验错误。
///
/// # 约束
/// 允许字母开头的 ASCII 字母数字、`_` 与 `-`，覆盖 `mall_order_fact` 与 `mall-snapshot`。
pub(super) fn parse_kind(value: &str, message: &str) -> Result<String> {
    parse_identifier(value, KIND_MAX_LEN, message)
}

/// 解析状态段。
///
/// # 参数
/// * `value` - 状态文本
///
/// # 返回
/// 返回已校验的状态段。
///
/// # 错误
/// 空、超长或非法标识时返回领域校验错误。
///
/// # 约束
/// 与类型段同一标识规则，避免状态被当成新的分隔字段。
pub(super) fn parse_status(value: &str) -> Result<String> {
    parse_identifier(value, STATUS_MAX_LEN, "证据记录 ID 必须使用唯一的 type:id 格式")
}

/// 解析标识段。
///
/// # 参数
/// * `value` - 原始段
/// * `max_len` - 最大长度
/// * `message` - 失败消息
///
/// # 返回
/// 返回已校验文本。
///
/// # 错误
/// 不符合标识规则时返回领域校验错误。
///
/// # 约束
/// 首字符必须是 ASCII 字母。
fn parse_identifier(value: &str, max_len: usize, message: &str) -> Result<String> {
    let mut characters = value.chars();
    if value.len() > max_len
        || !characters.next().is_some_and(|first| first.is_ascii_alphabetic())
        || !characters
            .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return Err(Error::from(message));
    }
    Ok(value.to_string())
}

/// 解析身份 ID。
///
/// # 参数
/// * `value` - 原始 ID
/// * `message` - 失败消息
///
/// # 返回
/// 返回已校验 ID。
///
/// # 错误
/// 空、超长或含分隔符/空白时返回领域校验错误。
///
/// # 约束
/// 允许 Unicode，但禁止 `:` `/` `;` `|` `,` `=` 与空白，杜绝集合拆分注入。
pub(super) fn parse_id(value: &str, message: &str) -> Result<String> {
    if value.is_empty() || value.len() > ID_MAX_LEN || value.chars().any(is_delimiter) {
        return Err(Error::from(message));
    }
    Ok(value.to_string())
}

/// 解析 canonical 版本段。
///
/// # 参数
/// * `value` - `v` 前缀加十进制数字
///
/// # 返回
/// 返回版本号。
///
/// # 错误
/// 缺少 `v`、非数字、前导零或溢出时返回领域校验错误。
///
/// # 约束
/// `v0` 合法；`v01` 非法，避免同一版本多种写法。
pub(super) fn parse_version_token(value: &str) -> Result<u64> {
    let digits =
        value.strip_prefix('v').ok_or_else(|| Error::from("证据记录 ID 必须使用唯一的 type:id 格式"))?;
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return Err(Error::from("证据记录 ID 必须使用唯一的 type:id 格式"));
    }
    digits.parse().map_err(|_| Error::from("证据记录 ID 必须使用唯一的 type:id 格式"))
}

/// 判断字符是否会切断引用边界。
///
/// # 参数
/// * `character` - 待检查字符
///
/// # 返回
/// 空白或冻结分隔符时返回 `true`。
///
/// # 错误
/// 无。
///
/// # 约束
/// 与历史 substring 拆分字符对齐，但只用于拒绝而不是命中。
fn is_delimiter(character: char) -> bool {
    character.is_whitespace() || DELIMITERS.contains(&character)
}

#[cfg(test)]
mod tests {
    use super::{encode_set, parse_kind, parse_status};

    #[test]
    fn identifiers_preserve_ascii_grammar_and_length_limits() {
        for value in ["a", "A_0-z", &"a".repeat(64)] {
            assert_eq!(parse_kind(value, "类型非法").unwrap(), value);
            assert_eq!(parse_status(value).unwrap(), value);
        }
        for value in ["", "0kind", "_kind", "a b", "a:kind", "é", "a中", &"a".repeat(65)] {
            assert_eq!(parse_kind(value, "类型非法").unwrap_err().to_string(), "类型非法");
            assert_eq!(
                parse_status(value).unwrap_err().to_string(),
                "证据记录 ID 必须使用唯一的 type:id 格式",
            );
        }
    }

    #[test]
    fn encoded_sets_preserve_empty_policy_order_deduplication_and_byte_limit() {
        assert_eq!(encode_set([], ',', false, "集合非法").unwrap(), "");
        assert_eq!(encode_set([], ';', true, "集合非法").unwrap_err().to_string(), "集合非法");
        assert_eq!(encode_set(["b".into(), "a".into(), "b".into()], ';', true, "集合非法").unwrap(), "a;b",);
        assert_eq!(encode_set(["a".repeat(512)], ';', true, "集合非法").unwrap().len(), 512);
        assert_eq!(encode_set(["a".repeat(513)], ';', true, "集合非法").unwrap_err().to_string(), "集合非法",);
        assert!(encode_set(["中".repeat(171)], ';', true, "集合非法").is_err());
    }
}
