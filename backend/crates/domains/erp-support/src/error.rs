//! 支撑领域应用错误，保留原服务层映射。

use application_core::{ValidationIssues, impl_application_error_conversions, impl_domain_error_class};
use persistence_core::impl_repository_error;

/// 支撑领域结果别名。
pub type Result<T> = std::result::Result<T, Error>;

impl_application_error_conversions!(Error);

/// 来源注册、批量任务与文件资产的错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("系统内部错误: {0}")]
    Internal(String),

    #[error("数据不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    ValidationError(String),

    /// 字段校验明细，跨边界不得压成字符串。
    #[error(transparent)]
    InvalidInput(#[from] ValidationIssues),

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

impl_domain_error_class!(Error);

impl_repository_error!(
    Error,
    duplicate =
        |error| { Error::ConflictError(duplicate_key_conflict_message(error.duplicate_index_name())) }
);

/// 将唯一键冲突映射为面向用户的冲突提示。
///
/// 按索引名做最小映射：`request_id`/`job_no` 与外部身份键给出各自的冲突
/// 文案，未知索引回退到通用「数据已存在」文案（原先走旧 `services::Error`
/// 的未知索引分支，保持 HTTP 冲突提示不变）。`Error::class` 保持 `Conflict`。
///
/// # 参数
/// * `index` - `persistence_core::Error::duplicate_index_name` 提取的索引名；
///   `None` 表示无法识别，走通用文案。
fn duplicate_key_conflict_message(index: Option<&str>) -> String {
    let index = index.unwrap_or_default();
    if index.contains("request_id") {
        return "相同请求已提交，请勿重复提交".to_string();
    }
    if index.contains("jobs_no") {
        return "任务编号已存在，请勿重复提交".to_string();
    }
    if index.contains("external_identity") {
        return "外部身份映射已存在，请勿重复提交".to_string();
    }
    if index.contains("storage_key") {
        return "同一文件已登记，请勿重复提交".to_string();
    }
    "数据已存在，请勿重复提交".to_string()
}

#[cfg(test)]
mod tests {
    use application_core::ErrorClass;
    use mongodb::error::Error as MongoError;

    use super::Error;

    #[test]
    fn external_identity_unique_conflicts_keep_conflict_class() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "E11000 duplicate key error collection: erp.external_identity_maps index: uk_external_identity_maps_identity",
        )));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(
            super::duplicate_key_conflict_message(Some("uk_external_identity_maps_identity")),
            "外部身份映射已存在，请勿重复提交"
        );
    }

    #[test]
    fn known_conflict_indexes_map_to_specific_messages() {
        let conflict = |index: &str| super::duplicate_key_conflict_message(Some(index));
        assert_eq!(conflict("uk_background_jobs_request_id"), "相同请求已提交，请勿重复提交");
        assert_eq!(conflict("uk_background_jobs_no"), "任务编号已存在，请勿重复提交");
        assert_eq!(conflict("uk_file_assets_storage_key"), "同一文件已登记，请勿重复提交");
        assert_eq!(super::duplicate_key_conflict_message(None), "数据已存在，请勿重复提交");
    }

    #[test]
    fn unknown_conflict_index_falls_back_to_generic_message() {
        let error = Error::from(persistence_core::Error::DuplicateKey(MongoError::custom(
            "E11000 duplicate key error collection: erp.jobs index: uk_some_future_index",
        )));
        assert_eq!(error.class(), ErrorClass::Conflict);
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }
}
