//! 只读命令查证结果与首次提交错误的合并合同。

/// 合并原命令的只读查证结果，不执行或重放业务写入。
///
/// # 参数
/// * `original` - 首次业务命令返回的错误，保留原类型及载荷。
/// * `recovered` - 原命令身份对应的只读结果查证。
/// * `is_unknown` - 由消费方判断原错误是否表示提交结果未知。
///
/// # 返回
/// 查证命中时返回已经持久化的原结果。
///
/// # 错误
/// 未查到结果时保留首次错误；查证失败时保留首次未知提交错误，其余返回查证错误。
pub fn recover_resource<T, E>(
    original: E,
    recovered: std::result::Result<Option<T>, E>,
    is_unknown: impl FnOnce(&E) -> bool,
) -> std::result::Result<T, E> {
    match recovered {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(original),
        Err(_) if is_unknown(&original) => Err(original),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::recover_resource;

    #[derive(Clone, Debug, PartialEq)]
    enum Failure {
        Unknown(&'static str),
        Rejected(&'static str),
    }

    fn unknown(error: &Failure) -> bool {
        matches!(error, Failure::Unknown(_))
    }

    #[test]
    fn committed_result_is_returned_after_known_or_unknown_failure() {
        for original in [Failure::Unknown("commit"), Failure::Rejected("race")] {
            assert_eq!(recover_resource(original, Ok(Some(17_u32)), unknown).unwrap(), 17);
        }
    }

    #[test]
    fn missing_result_keeps_original_error() {
        for original in [Failure::Unknown("commit"), Failure::Rejected("race")] {
            let expected = original.clone();
            assert_eq!(recover_resource::<u32, _>(original, Ok(None), unknown), Err(expected));
        }
    }

    #[test]
    fn lookup_failure_preserves_unknown_commit_and_otherwise_returns_lookup_error() {
        assert_eq!(
            recover_resource::<u32, _>(Failure::Unknown("commit"), Err(Failure::Rejected("lookup")), unknown),
            Err(Failure::Unknown("commit")),
        );
        assert_eq!(
            recover_resource::<u32, _>(Failure::Rejected("race"), Err(Failure::Rejected("lookup")), unknown),
            Err(Failure::Rejected("lookup")),
        );
    }
}
