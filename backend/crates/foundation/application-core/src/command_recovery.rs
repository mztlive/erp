//! 只读命令查证结果与首次提交错误的合并合同。

/// 首次写入返回错误后，用只读查证命中的持久化结果恢复响应，不执行或重放写入。未查到时保留首次错误；
/// 查证失败时，仅在调用方判定首次提交结果未知的情况下保留首次错误，否则返回查证错误。
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
