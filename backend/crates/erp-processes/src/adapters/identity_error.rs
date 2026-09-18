//! 范围 adapter 共用的身份错误映射。
//!
//! 各 `*_data_scope` adapter 把身份域变体同构搬到消费方领域；唯一特例是
//! `Rbac` 归入 `Internal`，因为消费方领域没有 RBAC 变体。不得经
//! `crate::Error` 再转回去。

/// 生成将身份域错误映射为 `$domain::Error` 的 `map_identity_error`。
///
/// # 参数
/// * `$domain` - 消费方 crate 标识符（如 `erp_customer`）
///
/// # 返回
/// 展开为模块内 `fn map_identity_error`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `Rbac` 必须映射为 `Internal`；不得把 Forbidden 改写成空集。
macro_rules! map_identity_error {
    ($domain:ident) => {
        /// 将身份域错误映射为消费方领域错误。
        ///
        /// # 参数
        /// * `error` - 身份域错误
        ///
        /// # 返回
        /// 返回同构载荷的领域错误；RBAC 内部失败归入系统错误。
        ///
        /// # 错误
        /// 无。
        ///
        /// # 关键业务约束
        /// 不得把身份域 Forbidden 改写成校验通过后的空集。
        fn map_identity_error(error: erp_identity::Error) -> $domain::Error {
            match error {
                erp_identity::Error::Internal(payload) => $domain::Error::Internal(payload),
                erp_identity::Error::NotFound(payload) => $domain::Error::NotFound(payload),
                erp_identity::Error::ValidationError(payload) => $domain::Error::ValidationError(payload),
                erp_identity::Error::BusinessLogicError(payload) => {
                    $domain::Error::BusinessLogicError(payload)
                },
                erp_identity::Error::ConflictError(payload) => $domain::Error::ConflictError(payload),
                erp_identity::Error::ReceiptDuplicate(payload) => $domain::Error::ReceiptDuplicate(payload),
                erp_identity::Error::TransientTransaction(payload) => {
                    $domain::Error::TransientTransaction(payload)
                },
                erp_identity::Error::Forbidden(payload) => $domain::Error::Forbidden(payload),
                erp_identity::Error::Unauthenticated(payload) => $domain::Error::Unauthenticated(payload),
                erp_identity::Error::Logic(payload) => $domain::Error::Logic(payload),
                erp_identity::Error::Rbac(payload) => $domain::Error::Internal(payload),
                erp_identity::Error::OutcomeUnknown(payload) => $domain::Error::OutcomeUnknown(payload),
                erp_identity::Error::RepositoryError(payload) => $domain::Error::RepositoryError(payload),
            }
        }
    };
}
pub(crate) use map_identity_error;

#[cfg(test)]
mod tests {
    map_identity_error!(erp_customer);

    #[test]
    fn identity_forbidden_stays_forbidden() {
        match map_identity_error(erp_identity::Error::Forbidden("没有该资源动作权限".into())) {
            erp_customer::Error::Forbidden(message) => {
                assert_eq!(message, "没有该资源动作权限");
            },
            other => panic!("expected forbidden, got {other:?}"),
        }
    }

    #[test]
    fn identity_rbac_maps_to_internal() {
        match map_identity_error(erp_identity::Error::Rbac("策略快照失败".into())) {
            erp_customer::Error::Internal(message) => assert_eq!(message, "策略快照失败"),
            other => panic!("expected internal, got {other:?}"),
        }
    }
}
