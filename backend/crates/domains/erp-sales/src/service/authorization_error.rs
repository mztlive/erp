//! 身份域错误到销售领域错误的保真映射。
/// 保留身份域错误类别，不把设施故障转换为无权。
/// # 参数
/// error 为统一授权服务返回的身份域错误。
/// # 返回
/// 保留原语义的销售领域错误。
/// # 错误
/// 不执行 I/O。
pub(crate) fn identity_error(error: erp_identity::Error) -> crate::Error {
    match error {
        erp_identity::Error::Internal(v) | erp_identity::Error::Rbac(v) => crate::Error::Internal(v),
        erp_identity::Error::NotFound(v) => crate::Error::NotFound(v),
        erp_identity::Error::ValidationError(v) => crate::Error::ValidationError(v),
        erp_identity::Error::InvalidInput(v) => crate::Error::InvalidInput(v),
        erp_identity::Error::BusinessLogicError(v) => crate::Error::BusinessLogicError(v),
        erp_identity::Error::ConflictError(v) => crate::Error::ConflictError(v),
        erp_identity::Error::Forbidden(v) => crate::Error::Forbidden(v),
        erp_identity::Error::Unauthenticated(v) => crate::Error::Unauthenticated(v),
        erp_identity::Error::Logic(v) => crate::Error::Logic(v),
        erp_identity::Error::ReceiptDuplicate(v) => crate::Error::ReceiptDuplicate(v),
        erp_identity::Error::TransientTransaction(v) => crate::Error::TransientTransaction(v),
        erp_identity::Error::OutcomeUnknown(v) => crate::Error::OutcomeUnknown(v),
        erp_identity::Error::RepositoryError(v) => crate::Error::RepositoryError(v),
    }
}

#[cfg(test)]
mod tests {
    use super::identity_error;
    use crate::Error;

    #[test]
    fn authorization_rejection_and_version_changes_keep_their_classes() {
        assert!(
            matches!(identity_error(erp_identity::Error::Forbidden("denied".into())), Error::Forbidden(message) if message == "denied")
        );
        assert!(
            matches!(identity_error(erp_identity::Error::ConflictError("DATA_SCOPE_CHANGED".into())), Error::ConflictError(message) if message == "DATA_SCOPE_CHANGED")
        );
    }

    #[test]
    fn identity_facility_errors_are_not_hidden_as_permission_rejections() {
        assert!(
            matches!(identity_error(erp_identity::Error::Rbac("policy unavailable".into())), Error::Internal(message) if message == "policy unavailable")
        );
        assert!(
            matches!(identity_error(erp_identity::Error::ValidationError("mapping missing".into())), Error::ValidationError(message) if message == "mapping missing")
        );
    }
}
