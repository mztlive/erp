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

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("认证失败: {0}")]
    Unauthenticated(String),

    #[error(transparent)]
    Logic(#[from] entities::Error),

    #[error("RBAC 错误: {0}")]
    Rbac(String),

    #[error("操作结果暂无法确认，请查询当前状态后再决定是否重试")]
    OutcomeUnknown(#[source] database::Error),

    #[error("数据库错误：{0}")]
    RepositoryError(database::Error),
}

impl From<database::Error> for Error {
    /// 将仓储错误转换为服务层错误。
    ///
    /// 唯一键、乐观锁和瞬态事务冲突保留为稳定的业务冲突语义，
    /// 其余错误保持内部仓储错误。唯一键冲突优先按已知索引名给出
    /// 面向用户的字段级提示，避免一律返回笼统的「数据已存在」。
    fn from(error: database::Error) -> Self {
        match error {
            error @ database::Error::DuplicateKey(_) => {
                Self::ConflictError(duplicate_key_conflict_message(&error))
            }
            database::Error::OptimisticLockingError => {
                Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
            }
            database::Error::TransientTransactionConflict(_) => {
                Self::ConflictError("并发事务冲突，请重试".to_string())
            }
            error @ database::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
            other => Self::RepositoryError(other),
        }
    }
}

/// 将唯一键冲突映射为面向用户的冲突提示。
///
/// # 参数
/// * `error` - 已归类为 `DuplicateKey` 的仓储错误
///
/// # 返回
/// 已知索引返回字段级中文提示；无法识别时返回通用冲突提示。
fn duplicate_key_conflict_message(error: &database::Error) -> String {
    match error.duplicate_index_name() {
        Some("uk_parties_party_no") => "主体编号已存在".to_string(),
        Some("uk_parties_credit_code") => "统一社会信用代码已存在".to_string(),
        Some("uk_party_bank_accounts_bank_account_no") => "银行账户编号已存在".to_string(),
        Some("uk_party_bank_accounts_party_hmac") => "该主体下银行账号已存在".to_string(),
        Some("uk_supplier_accounts_party") => "该主体已绑定供应商角色".to_string(),
        Some("uk_supplier_accounts_supplier_no") => "供应商编号已存在".to_string(),
        Some("uk_supplier_offerings_supplier_sku") => "该供应商 SKU 已登记供给".to_string(),
        Some("uk_customer_accounts_party") => "该主体已绑定客户角色".to_string(),
        Some("uk_customer_accounts_customer_no") => "客户编号已存在".to_string(),
        Some("uk_procurement_confirmation_lines_confirmation_line")
        | Some("uk_procurement_confirmation_lines_active_confirmation_line") => {
            "该采购确认已有相同分行序号，请刷新后重试".to_string()
        }
        _ => "数据已存在，请勿重复提交".to_string(),
    }
}

impl From<validator::ValidationErrors> for Error {
    /// 从给定值构建实例。
    ///
    /// # 参数
    /// * `err` - 错误对象
    ///
    /// # 返回
    /// 返回创建的实例。
    fn from(err: validator::ValidationErrors) -> Self {
        Error::ValidationError(err.to_string())
    }
}

/// 合同冻结的审批稳定错误码。HTTP 与 readiness 只识别这些字面量。
pub mod approval_codes {
    /// 固定类型缺少政策；只允许映射为内部错误并使启动/readiness 失败。
    pub const POLICY_NOT_REGISTERED: &str = "APPROVAL_POLICY_NOT_REGISTERED";
    /// 必须审批但无可绑定发布定义。
    pub const PROCESS_NOT_CONFIGURED: &str = "APPROVAL_PROCESS_NOT_CONFIGURED";
    /// 请求从当前发布版本创建草稿，但当前无发布定义。
    pub const DRAFT_SOURCE_NOT_AVAILABLE: &str = "APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE";
    /// 修改非草稿定义。
    pub const DEFINITION_NOT_DRAFT: &str = "APPROVAL_DEFINITION_NOT_DRAFT";
    /// 定义锁版本过期。
    pub const DEFINITION_VERSION_CONFLICT: &str = "APPROVAL_DEFINITION_VERSION_CONFLICT";
    /// 图、节点、人员或动作校验失败。
    pub const DEFINITION_INVALID: &str = "APPROVAL_DEFINITION_INVALID";
    /// 单据绑定缺失或不一致。
    pub const DEFINITION_BINDING_CORRUPTED: &str = "APPROVAL_DEFINITION_BINDING_CORRUPTED";
    /// 同一提交版本已有非终态实例。
    pub const ALREADY_STARTED: &str = "APPROVAL_ALREADY_STARTED";
    /// 任务已完成或关闭。
    pub const TASK_NOT_OPEN: &str = "APPROVAL_TASK_NOT_OPEN";
    /// 当前用户不是三方一致责任人。
    pub const TASK_NOT_ASSIGNED_TO_ACTOR: &str = "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR";
    /// 任务版本过期。
    pub const TASK_VERSION_CONFLICT: &str = "APPROVAL_TASK_VERSION_CONFLICT";
    /// 实例并发变化。
    pub const INSTANCE_VERSION_CONFLICT: &str = "APPROVAL_INSTANCE_VERSION_CONFLICT";
    /// 节点执行并发变化。
    pub const EXECUTION_VERSION_CONFLICT: &str = "APPROVAL_EXECUTION_VERSION_CONFLICT";
    /// 单据提交版本不一致。
    pub const SUBJECT_VERSION_CONFLICT: &str = "APPROVAL_SUBJECT_VERSION_CONFLICT";
    /// 驳回原因为空。
    pub const REJECT_REASON_REQUIRED: &str = "APPROVAL_REJECT_REASON_REQUIRED";
    /// 当前实例已受阻，不能决定。
    pub const INSTANCE_BLOCKED: &str = "APPROVAL_INSTANCE_BLOCKED";
    /// 当前 blocker 不属于人员失效，不能恢复。
    pub const RESUME_NOT_ALLOWED_FOR_BLOCKER: &str = "APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER";
    /// 恢复时原审批人仍不合格。
    pub const CURRENT_APPROVER_NOT_RECOVERED: &str = "APPROVAL_CURRENT_APPROVER_NOT_RECOVERED";
    /// 改派时原审批人已经恢复。
    pub const CURRENT_APPROVER_RECOVERED: &str = "APPROVAL_CURRENT_APPROVER_RECOVERED";
    /// 改派目标不满足资格。
    pub const REASSIGN_TARGET_INELIGIBLE: &str = "APPROVAL_REASSIGN_TARGET_INELIGIBLE";
    /// 当前 blocker 不属于人员失效，不能改派。
    pub const REASSIGN_NOT_ALLOWED_FOR_BLOCKER: &str = "APPROVAL_REASSIGN_NOT_ALLOWED_FOR_BLOCKER";
    /// 当前 blocker 属于人员失效，不能受阻取消。
    pub const BLOCKED_CANCEL_NOT_ALLOWED: &str = "APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED";
    /// 通用 WorkItem 命令试图修改审批任务。
    pub const GENERIC_WORK_ITEM_MUTATION_FORBIDDEN: &str = "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN";
    /// 同幂等键不同 canonical payload。
    pub const IDEMPOTENCY_PAYLOAD_CONFLICT: &str = "APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT";
    /// 未完成目标 rollout 的必须审批类型，不得回退旧运行时。
    pub const DOCUMENT_TYPE_NOT_CUT_OVER: &str = "APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER";

    /// 合同冻结的全部审批稳定码，供提取与测试穷尽。
    pub const ALL: &[&str] = &[
        POLICY_NOT_REGISTERED,
        PROCESS_NOT_CONFIGURED,
        DRAFT_SOURCE_NOT_AVAILABLE,
        DEFINITION_NOT_DRAFT,
        DEFINITION_VERSION_CONFLICT,
        DEFINITION_INVALID,
        DEFINITION_BINDING_CORRUPTED,
        ALREADY_STARTED,
        TASK_NOT_OPEN,
        TASK_NOT_ASSIGNED_TO_ACTOR,
        TASK_VERSION_CONFLICT,
        INSTANCE_VERSION_CONFLICT,
        EXECUTION_VERSION_CONFLICT,
        SUBJECT_VERSION_CONFLICT,
        REJECT_REASON_REQUIRED,
        INSTANCE_BLOCKED,
        RESUME_NOT_ALLOWED_FOR_BLOCKER,
        CURRENT_APPROVER_NOT_RECOVERED,
        CURRENT_APPROVER_RECOVERED,
        REASSIGN_TARGET_INELIGIBLE,
        REASSIGN_NOT_ALLOWED_FOR_BLOCKER,
        BLOCKED_CANCEL_NOT_ALLOWED,
        GENERIC_WORK_ITEM_MUTATION_FORBIDDEN,
        IDEMPOTENCY_PAYLOAD_CONFLICT,
        DOCUMENT_TYPE_NOT_CUT_OVER,
    ];
}

impl Error {
    /// 由合同冻结的审批稳定码构造服务错误。
    ///
    /// `APPROVAL_POLICY_NOT_REGISTERED` 只允许作为内部错误；资格与图校验走 422 语义，
    /// 责任不匹配走 403，其余稳定码走冲突。未接入类型不得回退旧运行时。
    ///
    /// # 参数
    /// * `code` - 合同冻结的 `APPROVAL_*` 码
    ///
    /// # 返回
    /// 返回已带稳定码的服务错误。
    pub fn from_approval_code(code: &'static str) -> Self {
        match code {
            approval_codes::POLICY_NOT_REGISTERED => Self::Internal(code.to_string()),
            approval_codes::TASK_NOT_ASSIGNED_TO_ACTOR => Self::Forbidden(code.to_string()),
            approval_codes::DEFINITION_INVALID
            | approval_codes::REJECT_REASON_REQUIRED
            | approval_codes::REASSIGN_TARGET_INELIGIBLE => Self::ValidationError(code.to_string()),
            _ => Self::ConflictError(code.to_string()),
        }
    }

    /// 从错误文案提取合同审批稳定码。
    ///
    /// # 返回
    /// 命中冻结码时返回该字面量。
    pub fn approval_code(&self) -> Option<&'static str> {
        let message = self.to_string();
        approval_codes::ALL
            .iter()
            .copied()
            .find(|code| message.contains(code))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use super::{approval_codes, Error};

    #[test]
    fn optimistic_locking_error_maps_to_conflict() {
        let error = Error::from(database::Error::OptimisticLockingError);

        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn duplicate_key_error_maps_to_conflict() {
        let error = Error::from(database::Error::DuplicateKey(MongoError::custom("duplicate key")));

        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(error.to_string(), "数据冲突: 数据已存在，请勿重复提交");
    }

    #[test]
    fn known_party_duplicate_index_maps_to_field_message() {
        use mongodb::{
            bson::{doc, from_document},
            error::{ErrorKind, WriteError, WriteFailure},
        };

        let write_error: WriteError = from_document(doc! {
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": "E11000 duplicate key error collection: erp.parties index: uk_parties_party_no dup key: { party_no: \"PTY-1\" }",
            "errInfo": null,
        })
        .expect("write error fixture should deserialize");
        let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        let error = Error::from(database::Error::DuplicateKey(mongo_error));

        assert_eq!(error.to_string(), "数据冲突: 主体编号已存在");
    }

    #[test]
    fn transient_transaction_error_maps_to_conflict() {
        let error = Error::from(database::Error::TransientTransactionConflict(MongoError::custom(
            "write conflict",
        )));

        assert!(matches!(error, Error::ConflictError(_)));
    }

    #[test]
    fn other_database_error_remains_repository_error() {
        let error = Error::from(database::Error::DatabaseError(MongoError::custom(
            "connection failed",
        )));

        assert!(matches!(error, Error::RepositoryError(_)));
    }

    #[test]
    fn unknown_commit_outcome_has_dedicated_service_semantics() {
        let error = Error::from(database::Error::CommitOutcomeUnknown(MongoError::custom(
            "unknown commit",
        )));

        assert!(matches!(&error, Error::OutcomeUnknown(_)));
        assert_eq!(
            error.to_string(),
            "操作结果暂无法确认，请查询当前状态后再决定是否重试"
        );
    }

    #[test]
    fn approval_policy_not_registered_is_internal() {
        let error = Error::from_approval_code(approval_codes::POLICY_NOT_REGISTERED);
        assert!(matches!(error, Error::Internal(_)));
        assert_eq!(error.approval_code(), Some(approval_codes::POLICY_NOT_REGISTERED));
    }

    #[test]
    fn uncut_document_type_is_conflict_and_not_legacy_fallback() {
        let error = Error::from_approval_code(approval_codes::DOCUMENT_TYPE_NOT_CUT_OVER);
        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(
            error.approval_code(),
            Some(approval_codes::DOCUMENT_TYPE_NOT_CUT_OVER)
        );
    }

    #[test]
    fn approval_stable_codes_are_exhaustive() {
        assert_eq!(approval_codes::ALL.len(), 25);
        assert!(approval_codes::ALL
            .iter()
            .all(|code| code.starts_with("APPROVAL_")));
    }
}
