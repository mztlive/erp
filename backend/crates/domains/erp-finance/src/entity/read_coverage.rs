//! 完整财务单据读取必须同时具备整账职责及每一实际来源的访问权。

/// 判定完整款项、票据或成本的读取资格。
///
/// # 参数
/// * `ledger_read` - 当前有效的显式财务整账读取职责。
/// * `sources` - 原单每条分配的真实来源；`None` 表示关联损坏。
/// * `visible` - 已按各自来源类型验证的来源主键集合。
///
/// # 返回
/// 显式整账职责且全部实际来源可见时返回 `true`；真正零分配仅要求整账职责。
///
/// # 错误
/// 不返回错误。来源缺失时返回 `false`，不能当成零分配。
pub fn whole_document_readable<'a>(
    ledger_read: bool,
    mut sources: impl Iterator<Item = Option<&'a str>>,
    visible: &[String],
) -> bool {
    ledger_read && sources.all(|source| source.is_some_and(|id| visible.iter().any(|item| item == id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unallocated_requires_ledger_duty_without_inventing_a_source_scope() {
        assert!(whole_document_readable(true, std::iter::empty(), &[]));
        assert!(!whole_document_readable(false, std::iter::empty(), &[]));
    }

    #[test]
    fn partial_source_and_dangling_reference_never_authorize_whole_document() {
        let visible = vec!["one".into()];
        assert!(whole_document_readable(true, [Some("one")].into_iter(), &visible));
        assert!(!whole_document_readable(false, [Some("one")].into_iter(), &visible));
        assert!(!whole_document_readable(true, [Some("one"), Some("two")].into_iter(), &visible));
        assert!(!whole_document_readable(true, [Some("one"), None].into_iter(), &visible));
    }
}
