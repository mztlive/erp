//! W13 卡券票款命令身份与版本锁（FIN-E11）。
//!
//! 将 `validate_card_funds_work_item` 与登记上下文中的任务类型、对象、对象 ID、
//! 对象版本、任务版本、开放状态与当前责任人比对收敛为 WorkItem 领域方法。
//! 方法只解释已装载任务事实，不查询 RBAC、DataScope 或队列。

use super::{WorkItem, WorkItemStatus, WorkItemType};
use crate::ids::ReceivableAccountId;

const RECEIVABLE_OBJECT_TYPE: &str = "receivable_account";

/// W13 卡券票款复核种类（期初或同步差额）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsReviewKind {
    /// 卡券期初票款复核，对应 [`WorkItemType::CardFundsReview`]。
    Opening,
    /// 同步差额复核，对应 [`WorkItemType::CardFundsDeltaReview`]。
    SyncDelta,
}

impl CardFundsReviewKind {
    /// 返回该复核种类必须绑定的任务类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 期初返回 `CardFundsReview`，差额返回 `CardFundsDeltaReview`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 纯映射，不读取任务或账户状态。
    pub fn work_item_type(self) -> WorkItemType {
        match self {
            Self::Opening => WorkItemType::CardFundsReview,
            Self::SyncDelta => WorkItemType::CardFundsDeltaReview,
        }
    }
}

/// W13 命令对任务身份的期望事实。
///
/// 登记命令接受任一 W13 任务类型，并由任务对象 ID 定位账户；正式复核命令
/// 必须同时锁定复核种类、应收账户 ID 与任务自身 ID（复核身份）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardFundsCommandSubject<'a> {
    /// 历史票款登记：任务必须是应收账户上的开放 W13 任务。
    Registration {
        /// 已授权操作人。
        actor_id: &'a str,
        /// 调用方持有的任务乐观锁版本。
        expected_task_version: u64,
        /// 调用方持有的销售对象版本。
        expected_subject_version: &'a str,
    },
    /// 正式复核决定：任务必须是指定账户、指定复核种类的当前开放任务。
    Review {
        /// 已授权操作人。
        actor_id: &'a str,
        /// 调用方持有的任务乐观锁版本。
        expected_task_version: u64,
        /// 调用方持有的销售对象版本。
        expected_subject_version: &'a str,
        /// 决定绑定的应收账户。
        expected_account_id: &'a ReceivableAccountId,
        /// 决定绑定的复核种类。
        expected_review_kind: CardFundsReviewKind,
        /// 决定绑定的复核任务 ID。
        expected_review_id: &'a str,
    },
}

/// 已通过 W13 身份与版本锁的任务快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsCommandLock<'a> {
    work_item: &'a WorkItem,
}

impl<'a> CardFundsCommandLock<'a> {
    /// 返回已锁定任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回通过身份校验的任务引用。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 不复制任务，不改变任务状态。
    pub fn work_item(&self) -> &'a WorkItem {
        self.work_item
    }

    /// 返回任务绑定的应收账户 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回任务冻结的业务对象 ID。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方仍须按该 ID 装载账户事实。
    pub fn receivable_account_id(&self) -> &'a str {
        self.work_item.business_object_id.as_str()
    }

    /// 返回已锁定的 W13 任务类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `CardFundsReview` 或 `CardFundsDeltaReview`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 构造时已收窄为这两种类型。
    pub fn work_item_type(&self) -> WorkItemType {
        self.work_item.work_item_type
    }
}

/// W13 命令身份／版本锁失败原因。
///
/// 文案与原 Service 私有 helper 保持一致，由 Service 映射为 Conflict / 业务 /
/// Forbidden，禁止把实体 `LogicError` 直接当作 HTTP 合同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CardFundsCommandIdentityError {
    /// 任务乐观锁版本与调用方期望不一致。
    #[error("复核任务版本已变化，请刷新后重试")]
    TaskVersionChanged,
    /// 任务冻结的销售对象版本与调用方期望不一致。
    #[error("复核任务对象版本已变化，请刷新后重试")]
    SubjectVersionChanged,
    /// 登记命令遇到非应收账户 W13 任务。
    #[error("当前任务不是应收账户卡券票款复核任务")]
    NotRegistrationTask,
    /// 正式复核命令遇到对象类型或账户 ID 不一致。
    #[error("当前任务不是该应收账户的独立票款复核任务")]
    NotReviewTask,
    /// 正式复核种类与任务类型不一致。
    #[error("复核类型与任务类型不一致")]
    ReviewTypeMismatch,
    /// 任务 ID 与决定绑定的复核身份不一致。
    #[error("当前任务不是该应收账户的独立票款复核任务")]
    ReviewIdentityMismatch,
    /// 当前账号不是开放任务的当前责任人，或任务已非开放。
    #[error("当前账号不是开放任务的当前责任人")]
    NotCurrentOpenOwner,
}

impl WorkItem {
    /// 校验本任务可作为 W13 命令的身份与版本锁。
    ///
    /// 接收已授权 actor 与期望对象事实，按固定首错顺序比对任务版本、对象版本、
    /// 对象类型／对象 ID、复核种类、复核任务 ID、开放状态与当前责任人。
    ///
    /// # 参数
    /// * `subject` - 登记或正式复核的期望身份事实
    ///
    /// # 返回
    /// 全部身份字段一致时返回 [`CardFundsCommandLock`]。
    ///
    /// # 错误
    /// 任一字段不符即 fail-closed，返回对应 [`CardFundsCommandIdentityError`]；
    /// 不修改任务。
    ///
    /// # 约束
    /// 不查询权限存储、责任规则或队列；RBAC／DataScope 仍由 Service 执行。
    pub fn lock_card_funds_command<'a>(
        &'a self,
        subject: CardFundsCommandSubject<'_>,
    ) -> std::result::Result<CardFundsCommandLock<'a>, CardFundsCommandIdentityError> {
        match subject {
            CardFundsCommandSubject::Registration {
                actor_id,
                expected_task_version,
                expected_subject_version,
            } => self.lock_registration(actor_id, expected_task_version, expected_subject_version),
            CardFundsCommandSubject::Review {
                actor_id,
                expected_task_version,
                expected_subject_version,
                expected_account_id,
                expected_review_kind,
                expected_review_id,
            } => self.lock_review(
                actor_id,
                expected_task_version,
                expected_subject_version,
                expected_account_id,
                expected_review_kind,
                expected_review_id,
            ),
        }
    }

    /// 按登记命令合同锁定 W13 任务身份。
    ///
    /// # 参数
    /// * `actor_id` - 已授权操作人
    /// * `expected_task_version` - 期望任务版本
    /// * `expected_subject_version` - 期望对象版本
    ///
    /// # 返回
    /// 身份一致时返回锁。
    ///
    /// # 错误
    /// 首错顺序为任务版本、对象版本、对象类型／任务类型、当前开放责任。
    ///
    /// # 约束
    /// 不要求调用方提供账户 ID；账户由任务对象 ID 定位。
    fn lock_registration<'a>(
        &'a self,
        actor_id: &str,
        expected_task_version: u64,
        expected_subject_version: &str,
    ) -> std::result::Result<CardFundsCommandLock<'a>, CardFundsCommandIdentityError> {
        self.ensure_task_version(expected_task_version)?;
        self.ensure_subject_version(expected_subject_version)?;
        if self.business_object_type != RECEIVABLE_OBJECT_TYPE || !self.work_item_type.is_card_funds_review()
        {
            return Err(CardFundsCommandIdentityError::NotRegistrationTask);
        }
        self.ensure_current_open_owner(actor_id)?;
        Ok(CardFundsCommandLock { work_item: self })
    }

    /// 按正式复核命令合同锁定 W13 任务身份。
    ///
    /// # 参数
    /// * `actor_id` - 已授权操作人
    /// * `expected_task_version` - 期望任务版本
    /// * `expected_subject_version` - 期望对象版本
    /// * `expected_account_id` - 决定绑定的应收账户
    /// * `expected_review_kind` - 期初或差额
    /// * `expected_review_id` - 决定绑定的任务／复核身份
    ///
    /// # 返回
    /// 身份一致时返回锁。
    ///
    /// # 错误
    /// 首错顺序为任务版本、对象版本、对象类型／账户 ID、复核种类、复核 ID、
    /// 当前开放责任。
    ///
    /// # 约束
    /// 正确命令不得改写既有错误文案。
    fn lock_review<'a>(
        &'a self,
        actor_id: &str,
        expected_task_version: u64,
        expected_subject_version: &str,
        expected_account_id: &ReceivableAccountId,
        expected_review_kind: CardFundsReviewKind,
        expected_review_id: &str,
    ) -> std::result::Result<CardFundsCommandLock<'a>, CardFundsCommandIdentityError> {
        self.ensure_task_version(expected_task_version)?;
        self.ensure_subject_version(expected_subject_version)?;
        if self.business_object_type != RECEIVABLE_OBJECT_TYPE
            || self.business_object_id != expected_account_id.as_ref()
        {
            return Err(CardFundsCommandIdentityError::NotReviewTask);
        }
        if self.work_item_type != expected_review_kind.work_item_type() {
            return Err(CardFundsCommandIdentityError::ReviewTypeMismatch);
        }
        if self.base.id != expected_review_id {
            return Err(CardFundsCommandIdentityError::ReviewIdentityMismatch);
        }
        self.ensure_current_open_owner(actor_id)?;
        Ok(CardFundsCommandLock { work_item: self })
    }

    /// 校验任务乐观锁版本。
    ///
    /// # 参数
    /// * `expected_task_version` - 调用方持有版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 不一致时返回 [`CardFundsCommandIdentityError::TaskVersionChanged`]。
    ///
    /// # 约束
    /// 不读取数据库。
    fn ensure_task_version(
        &self,
        expected_task_version: u64,
    ) -> std::result::Result<(), CardFundsCommandIdentityError> {
        if self.base.version != expected_task_version {
            return Err(CardFundsCommandIdentityError::TaskVersionChanged);
        }
        Ok(())
    }

    /// 校验任务冻结的对象版本。
    ///
    /// # 参数
    /// * `expected_subject_version` - 调用方持有的销售版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 不一致时返回 [`CardFundsCommandIdentityError::SubjectVersionChanged`]。
    ///
    /// # 约束
    /// 不做 trim；调用方传入已规范化期望值。
    fn ensure_subject_version(
        &self,
        expected_subject_version: &str,
    ) -> std::result::Result<(), CardFundsCommandIdentityError> {
        if self.subject_version != expected_subject_version {
            return Err(CardFundsCommandIdentityError::SubjectVersionChanged);
        }
        Ok(())
    }

    /// 校验任务仍开放且操作人是当前个人责任人。
    ///
    /// # 参数
    /// * `actor_id` - 已授权操作人
    ///
    /// # 返回
    /// 开放且责任匹配时返回 `Ok(())`。
    ///
    /// # 错误
    /// 非开放或非当前责任人时返回 [`CardFundsCommandIdentityError::NotCurrentOpenOwner`]。
    ///
    /// # 约束
    /// 与原 `is_owned_by` 合同一致，两种失败共用同一文案。
    fn ensure_current_open_owner(
        &self,
        actor_id: &str,
    ) -> std::result::Result<(), CardFundsCommandIdentityError> {
        if !self.is_owned_by(actor_id) {
            return Err(CardFundsCommandIdentityError::NotCurrentOpenOwner);
        }
        debug_assert_eq!(self.status, WorkItemStatus::Open);
        Ok(())
    }
}

impl WorkItemType {
    /// 判断任务类型是否为 W13 卡券票款复核。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 期初或差额复核返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 不包含其它财务任务类型。
    pub fn is_card_funds_review(self) -> bool {
        matches!(self, Self::CardFundsReview | Self::CardFundsDeltaReview)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CardFundsCommandIdentityError, CardFundsCommandSubject, CardFundsReviewKind, RECEIVABLE_OBJECT_TYPE,
    };
    use crate::common::time::Instant;
    use crate::ids::{ReceivableAccountId, WorkItemId};
    use crate::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

    fn opening_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-opening"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: RECEIVABLE_OBJECT_TYPE.to_string(),
                business_object_id: "ra-1".to_string(),
                subject_version: "sor-1".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "alice".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("CARD_FUNDS_OPENING_REVIEW".to_string()),
                impact_summary: Some("票款复核".to_string()),
            },
            Instant::from_unix_secs(1_700_000_000),
        )
        .unwrap()
    }

    fn delta_item() -> WorkItem {
        let mut item = opening_item();
        item.base.id = "wi-delta".to_string();
        item.work_item_type = WorkItemType::CardFundsDeltaReview;
        item
    }

    fn review_subject<'a>(
        actor_id: &'a str,
        account: &'a ReceivableAccountId,
        kind: CardFundsReviewKind,
        review_id: &'a str,
    ) -> CardFundsCommandSubject<'a> {
        CardFundsCommandSubject::Review {
            actor_id,
            expected_task_version: 1,
            expected_subject_version: "sor-1",
            expected_account_id: account,
            expected_review_kind: kind,
            expected_review_id: review_id,
        }
    }

    #[test]
    fn opening_and_delta_review_commands_lock() {
        let account = ReceivableAccountId::new("ra-1");
        let opening = opening_item();
        let lock = opening
            .lock_card_funds_command(review_subject(
                "alice",
                &account,
                CardFundsReviewKind::Opening,
                "wi-opening",
            ))
            .unwrap();
        assert_eq!(lock.receivable_account_id(), "ra-1");
        assert_eq!(lock.work_item_type(), WorkItemType::CardFundsReview);

        let delta = delta_item();
        let lock = delta
            .lock_card_funds_command(review_subject(
                "alice",
                &account,
                CardFundsReviewKind::SyncDelta,
                "wi-delta",
            ))
            .unwrap();
        assert_eq!(lock.work_item_type(), WorkItemType::CardFundsDeltaReview);
    }

    #[test]
    fn registration_accepts_either_w13_type() {
        let opening = opening_item();
        assert!(opening
            .lock_card_funds_command(CardFundsCommandSubject::Registration {
                actor_id: "alice",
                expected_task_version: 1,
                expected_subject_version: "sor-1",
            })
            .is_ok());
        let delta = delta_item();
        assert!(delta
            .lock_card_funds_command(CardFundsCommandSubject::Registration {
                actor_id: "alice",
                expected_task_version: 1,
                expected_subject_version: "sor-1",
            })
            .is_ok());
    }

    #[test]
    fn rejects_wrong_task_type_before_owner() {
        let account = ReceivableAccountId::new("ra-1");
        let opening = opening_item();
        let error = opening
            .lock_card_funds_command(review_subject(
                "bob",
                &account,
                CardFundsReviewKind::SyncDelta,
                "wi-opening",
            ))
            .unwrap_err();
        assert_eq!(error, CardFundsCommandIdentityError::ReviewTypeMismatch);
        assert_eq!(error.to_string(), "复核类型与任务类型不一致");
    }

    #[test]
    fn rejects_wrong_object_type_and_object_id() {
        let account = ReceivableAccountId::new("ra-1");
        let mut item = opening_item();
        item.business_object_type = "payable_account".to_string();
        assert_eq!(
            item.lock_card_funds_command(review_subject(
                "alice",
                &account,
                CardFundsReviewKind::Opening,
                "wi-opening",
            ))
            .unwrap_err(),
            CardFundsCommandIdentityError::NotReviewTask
        );

        let other = ReceivableAccountId::new("ra-2");
        let item = opening_item();
        assert_eq!(
            item.lock_card_funds_command(review_subject(
                "alice",
                &other,
                CardFundsReviewKind::Opening,
                "wi-opening",
            ))
            .unwrap_err(),
            CardFundsCommandIdentityError::NotReviewTask
        );
        assert_eq!(
            CardFundsCommandIdentityError::NotReviewTask.to_string(),
            "当前任务不是该应收账户的独立票款复核任务"
        );
    }

    #[test]
    fn rejects_wrong_subject_version_and_task_version() {
        let account = ReceivableAccountId::new("ra-1");
        let mut item = opening_item();
        item.base.version = 2;
        assert_eq!(
            item.lock_card_funds_command(review_subject(
                "alice",
                &account,
                CardFundsReviewKind::Opening,
                "wi-opening",
            ))
            .unwrap_err(),
            CardFundsCommandIdentityError::TaskVersionChanged
        );

        let item = opening_item();
        let error = item
            .lock_card_funds_command(CardFundsCommandSubject::Review {
                actor_id: "alice",
                expected_task_version: 1,
                expected_subject_version: "sor-2",
                expected_account_id: &account,
                expected_review_kind: CardFundsReviewKind::Opening,
                expected_review_id: "wi-opening",
            })
            .unwrap_err();
        assert_eq!(error, CardFundsCommandIdentityError::SubjectVersionChanged);
        assert_eq!(error.to_string(), "复核任务对象版本已变化，请刷新后重试");
    }

    #[test]
    fn rejects_wrong_review_id() {
        let account = ReceivableAccountId::new("ra-1");
        let item = opening_item();
        assert_eq!(
            item.lock_card_funds_command(review_subject(
                "alice",
                &account,
                CardFundsReviewKind::Opening,
                "wi-other",
            ))
            .unwrap_err(),
            CardFundsCommandIdentityError::ReviewIdentityMismatch
        );
    }

    #[test]
    fn rejects_non_owner_and_non_open() {
        let account = ReceivableAccountId::new("ra-1");
        let item = opening_item();
        assert_eq!(
            item.lock_card_funds_command(review_subject(
                "bob",
                &account,
                CardFundsReviewKind::Opening,
                "wi-opening",
            ))
            .unwrap_err(),
            CardFundsCommandIdentityError::NotCurrentOpenOwner
        );

        let mut closed = opening_item();
        closed
            .complete_by_domain_command("alice", Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(
            closed
                .lock_card_funds_command(review_subject(
                    "alice",
                    &account,
                    CardFundsReviewKind::Opening,
                    "wi-opening",
                ))
                .unwrap_err(),
            CardFundsCommandIdentityError::NotCurrentOpenOwner
        );
        assert_eq!(
            CardFundsCommandIdentityError::NotCurrentOpenOwner.to_string(),
            "当前账号不是开放任务的当前责任人"
        );
    }

    #[test]
    fn registration_rejects_non_w13_task() {
        let mut item = opening_item();
        item.work_item_type = WorkItemType::SalesInvoiceExecution;
        assert_eq!(
            item.lock_card_funds_command(CardFundsCommandSubject::Registration {
                actor_id: "alice",
                expected_task_version: 1,
                expected_subject_version: "sor-1",
            })
            .unwrap_err(),
            CardFundsCommandIdentityError::NotRegistrationTask
        );
        assert_eq!(
            CardFundsCommandIdentityError::NotRegistrationTask.to_string(),
            "当前任务不是应收账户卡券票款复核任务"
        );
    }

    #[test]
    fn valid_command_does_not_change_error_contract_text() {
        assert_eq!(
            CardFundsCommandIdentityError::TaskVersionChanged.to_string(),
            "复核任务版本已变化，请刷新后重试"
        );
    }
}
