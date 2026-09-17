//! 采购领域错误及原唯一索引冲突映射。

use application_core::ErrorClass;

/// Procurement-domain result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<application_core::Error> for Error {
    /// 将应用合同错误映射为采购领域错误。
    fn from(error: application_core::Error) -> Self {
        match error {
            application_core::Error::Internal(message) => Self::Internal(message),
            application_core::Error::ValidationError(message) => Self::ValidationError(message),
        }
    }
}

/// 采购订单与责任规则错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("数据不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    ValidationError(String),

    #[error("业务逻辑错误: {0}")]
    BusinessLogicError(String),

    #[error("数据冲突: {0}")]
    ConflictError(String),

    #[error("数据冲突: 数据已存在，请勿重复提交")]
    ReceiptDuplicate(#[source] persistence_core::Error),

    #[error("数据冲突: 并发事务冲突，请重试")]
    TransientTransaction(#[source] persistence_core::Error),

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("认证失败: {0}")]
    Unauthenticated(String),

    #[error(transparent)]
    Logic(#[from] erp_core::Error),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] persistence_core::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(persistence_core::Error),
}

impl Error {
    /// Stable error class used by HTTP mapping; do not parse display text.
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::Internal(_) | Self::Logic(_) => ErrorClass::Internal,
            Self::RepositoryError(error) => persistence_error_class(error),
            Self::ConflictError(_) => ErrorClass::Conflict,
            Self::ReceiptDuplicate(error) | Self::TransientTransaction(error) => {
                persistence_error_class(error)
            },
            Self::BusinessLogicError(_) | Self::ValidationError(_) | Self::NotFound(_) => {
                ErrorClass::BusinessRule
            },
            Self::Forbidden(_) | Self::Unauthenticated(_) => ErrorClass::Forbidden,
            Self::OutcomeUnknown(error) => persistence_error_class(error),
        }
    }

    /// 采购类型与创建依据不一致。
    ///
    /// # 返回
    /// 返回依据不一致校验错误（文案与历史调用点一致）。
    ///
    /// # 错误
    /// 本身即为错误值，不再失败。
    pub fn basis_purchase_type_mismatch() -> Self {
        Self::ValidationError("采购类型与创建依据不一致".to_string())
    }

    /// 付款条件与创建依据不一致。
    ///
    /// # 返回
    /// 返回依据不一致校验错误（文案与历史调用点一致）。
    ///
    /// # 错误
    /// 本身即为错误值，不再失败。
    pub fn basis_payment_term_mismatch() -> Self {
        Self::ValidationError("付款条件与创建依据不一致".to_string())
    }

    /// 采购预计交付日晚于销售承诺期限。
    ///
    /// # 参数
    /// * `sales_due` - 销售对客户承诺的最晚交付日
    ///
    /// # 返回
    /// 返回交付日越界校验错误（文案与历史调用点一致）。
    ///
    /// # 错误
    /// 本身即为错误值，不再失败。
    pub fn delivery_beyond_sales_due(sales_due: impl std::fmt::Display) -> Self {
        Self::ValidationError(format!("预计交付日不能晚于销售承诺期限 {sales_due}"))
    }
}

impl From<persistence_core::Error> for Error {
    /// 将仓储错误转换为采购领域错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。
    fn from(error: persistence_core::Error) -> Self {
        match error {
            error @ persistence_core::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(&error))
            },
            persistence_core::Error::OptimisticLockingError => {
                Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            },
            error @ persistence_core::Error::TransientTransactionConflict(_) => {
                Self::TransientTransaction(error)
            },
            error @ persistence_core::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::RepositoryError(other),
        }
    }
}

/// 将持久化源错误映射为稳定错误分类（唯一分类入口）。
///
/// 唯一键冲突、乐观锁与瞬态事务冲突为可重试/冲突语义；未知提交结果与其它
/// 仓储失败为内部错误。`Logic(erp_core::Error)` 不经过本函数：领域基元错误
/// 无稳定分类（字符串负载或状态迁移），保持 `Internal`。
/// 不新增错误变体：`web-api` 的 `boundary_error!` 按变体穷举映射，
/// 新增变体会破坏其穷举性（该 crate 不在本组可改范围）。
fn persistence_error_class(error: &persistence_core::Error) -> ErrorClass {
    match error {
        persistence_core::Error::DuplicateKey(_)
        | persistence_core::Error::OptimisticLockingError
        | persistence_core::Error::TransientTransactionConflict(_) => ErrorClass::Conflict,
        _ => ErrorClass::Internal,
    }
}

/// 将唯一键冲突映射为面向用户的冲突提示。
fn duplicate_key_conflict_message(error: &persistence_core::Error) -> String {
    duplicate_index_conflict_message(error.duplicate_index_name())
}

/// 将采购域唯一索引名称映射为面向用户的冲突提示。
///
/// 创建依据和责任选择器保留原专用文案，其余唯一索引使用原通用文案。
fn duplicate_index_conflict_message(index_name: Option<&str>) -> String {
    index_name.and_then(known_duplicate_index_message).unwrap_or("数据已存在，请勿重复提交").to_string()
}

/// 返回本域已知唯一索引的固定冲突提示。
///
/// # 参数
/// * `index_name` - 已由 persistence-core 提取的 MongoDB 索引名称
///
/// # 返回
/// 精确命中本域注册索引时返回原提示；未知名称返回 `None`，由调用边界选择通用提示。
///
/// # 约束
/// 不匹配子串或前后缀、不规范化索引名称，不拥有其他领域或 HTTP 历史索引的提示。
pub fn known_duplicate_index_message(index_name: &str) -> Option<&'static str> {
    match index_name {
        "uk_purchase_orders_creation_basis" => Some("该采购创建依据已生成采购单"),
        "uk_procurement_responsibility_active_selector" => Some("同一采购责任选择器只能有一条启用规则"),
        _ => None,
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从校验错误构建采购领域错误。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn procurement_unique_conflicts_keep_original_messages() {
        let generic = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key")));
        assert_eq!(generic.class(), ErrorClass::Conflict);
        assert_eq!(generic.to_string(), "数据冲突: 数据已存在，请勿重复提交");
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_purchase_orders_purchase_no")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_purchase_order_submissions_order_no")),
            "数据已存在，请勿重复提交"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_purchase_change_submissions_order_no")),
            "数据已存在，请勿重复提交"
        );
    }

    #[test]
    fn procurement_special_unique_messages_are_preserved() {
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_purchase_orders_creation_basis")),
            "该采购创建依据已生成采购单"
        );
        assert_eq!(
            super::duplicate_index_conflict_message(Some("uk_procurement_responsibility_active_selector")),
            "同一采购责任选择器只能有一条启用规则"
        );
    }

    #[test]
    fn optimistic_lock_maps_to_conflict_refresh_message() {
        let error = Error::from(persistence_core::Error::OptimisticLockingError);
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }

    #[test]
    fn persistence_wrappers_delegate_source_classification() {
        let duplicate = persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key"));
        assert_eq!(Error::ReceiptDuplicate(duplicate).class(), ErrorClass::Conflict);

        let transient = persistence_core::Error::TransientTransactionConflict(MongoError::custom("t"));
        assert_eq!(Error::TransientTransaction(transient).class(), ErrorClass::Conflict);

        let unknown = persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("u"));
        assert_eq!(Error::OutcomeUnknown(unknown).class(), ErrorClass::Internal);

        let database = persistence_core::Error::DatabaseError(MongoError::custom("db"));
        assert_eq!(Error::RepositoryError(database).class(), ErrorClass::Internal);
        let conflict_source = persistence_core::Error::DuplicateKey(MongoError::custom("duplicate key"));
        assert_eq!(Error::RepositoryError(conflict_source).class(), ErrorClass::Conflict);
    }

    #[test]
    fn named_basis_constructors_keep_original_texts_and_classes() {
        let type_mismatch = Error::basis_purchase_type_mismatch();
        assert_eq!(type_mismatch.to_string(), "参数验证失败: 采购类型与创建依据不一致");
        assert_eq!(type_mismatch.class(), ErrorClass::BusinessRule);

        let term_mismatch = Error::basis_payment_term_mismatch();
        assert_eq!(term_mismatch.to_string(), "参数验证失败: 付款条件与创建依据不一致");
        assert_eq!(term_mismatch.class(), ErrorClass::BusinessRule);

        let delivery = Error::delivery_beyond_sales_due("2026-10-31");
        assert_eq!(delivery.to_string(), "参数验证失败: 预计交付日不能晚于销售承诺期限 2026-10-31");
        assert_eq!(delivery.class(), ErrorClass::BusinessRule);
    }

    #[test]
    fn known_duplicate_export_keeps_exact_owned_index_matrix() {
        let cases = [
            ("uk_purchase_orders_creation_basis", "该采购创建依据已生成采购单"),
            ("uk_procurement_responsibility_active_selector", "同一采购责任选择器只能有一条启用规则"),
        ];
        for (index, expected) in cases {
            assert_eq!(super::known_duplicate_index_message(index), Some(expected));
            assert_eq!(super::known_duplicate_index_message(&format!("{index}_extra")), None);
            assert_eq!(super::known_duplicate_index_message(&format!(" {index}")), None);
            assert_eq!(super::known_duplicate_index_message(&index.to_ascii_uppercase()), None);
        }
        assert_eq!(super::known_duplicate_index_message(""), None);
        assert_eq!(super::known_duplicate_index_message("unknown_index"), None);
    }
}
