//! W13 任务身份锁的 Service 适配（FIN-E11）。
//!
//! 领域方法 [`entities::work_item::WorkItem::lock_card_funds_command`] 是唯一规则源；
//! 本文件只把类型化失败映射为既有 HTTP 错误合同。RBAC、DataScope 与队列查询仍在调用方。

use entities::ids::ReceivableAccountId;
use entities::work_item::{
    CardFundsCommandIdentityError, CardFundsCommandLock, CardFundsCommandSubject, CardFundsReviewKind,
    WorkItem,
};

use super::dto::CardFundsReviewType;
use crate::errors::{Error, Result};

/// 将 HTTP 复核类型映射为领域复核种类。
///
/// # 参数
/// * `review_type` - 服务 DTO 复核类型
///
/// # 返回
/// 期初或差额种类。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯枚举映射。
pub fn review_kind(review_type: CardFundsReviewType) -> CardFundsReviewKind {
    match review_type {
        CardFundsReviewType::Opening => CardFundsReviewKind::Opening,
        CardFundsReviewType::SyncDelta => CardFundsReviewKind::SyncDelta,
    }
}

/// 校验正式复核命令的 W13 任务身份与版本锁。
///
/// # 参数
/// * `item` - 事务内装载的任务
/// * `actor_id` - 已授权操作人
/// * `expected_task_version` - 期望任务版本
/// * `expected_subject_version` - 期望对象版本
/// * `expected_account_id` - 决定绑定的应收账户
/// * `review_type` - 决定绑定的复核种类
///
/// # 返回
/// 身份一致时返回锁。
///
/// # 错误
/// 按原 helper 映射为 Conflict／业务／Forbidden，文案不变。
///
/// # 约束
/// 不查询权限或队列。
pub fn lock_review_work_item<'a>(
    item: &'a WorkItem,
    actor_id: &'a str,
    expected_task_version: u64,
    expected_subject_version: &'a str,
    expected_account_id: &'a ReceivableAccountId,
    review_type: CardFundsReviewType,
) -> Result<CardFundsCommandLock<'a>> {
    item.lock_card_funds_command(CardFundsCommandSubject::Review {
        actor_id,
        expected_task_version,
        expected_subject_version,
        expected_account_id,
        expected_review_kind: review_kind(review_type),
        expected_review_id: item.base.id.as_str(),
    })
    .map_err(map_identity_error)
}

/// 校验历史票款登记命令的 W13 任务身份与版本锁。
///
/// # 参数
/// * `item` - 事务内装载的任务
/// * `actor_id` - 已授权操作人
/// * `expected_task_version` - 期望任务版本
/// * `expected_subject_version` - 已 trim 的期望对象版本
///
/// # 返回
/// 身份一致时返回锁。
///
/// # 错误
/// 按原登记上下文映射为 Conflict／业务／Forbidden。
///
/// # 约束
/// 账户 ID 由任务对象定位，不在此比对。
pub fn lock_registration_work_item<'a>(
    item: &'a WorkItem,
    actor_id: &'a str,
    expected_task_version: u64,
    expected_subject_version: &'a str,
) -> Result<CardFundsCommandLock<'a>> {
    item.lock_card_funds_command(CardFundsCommandSubject::Registration {
        actor_id,
        expected_task_version,
        expected_subject_version,
    })
    .map_err(map_identity_error)
}

/// 将领域身份错误映射为既有服务错误。
///
/// # 参数
/// * `error` - 领域失败原因
///
/// # 返回
/// 返回 Conflict、BusinessLogic 或 Forbidden。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 不得落到透明 `Error::Logic`，以免改变 HTTP 合同。
fn map_identity_error(error: CardFundsCommandIdentityError) -> Error {
    match error {
        CardFundsCommandIdentityError::TaskVersionChanged
        | CardFundsCommandIdentityError::SubjectVersionChanged => Error::ConflictError(error.to_string()),
        CardFundsCommandIdentityError::NotRegistrationTask
        | CardFundsCommandIdentityError::NotReviewTask
        | CardFundsCommandIdentityError::ReviewTypeMismatch
        | CardFundsCommandIdentityError::ReviewIdentityMismatch => {
            Error::BusinessLogicError(error.to_string())
        }
        CardFundsCommandIdentityError::NotCurrentOpenOwner => Error::Forbidden(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{lock_registration_work_item, lock_review_work_item, map_identity_error};
    use crate::errors::Error;
    use crate::receivable::dto::CardFundsReviewType;
    use entities::common::time::Instant;
    use entities::ids::{ReceivableAccountId, WorkItemId};
    use entities::work_item::{
        AssignmentSource, CardFundsCommandIdentityError, WorkItem, WorkItemData, WorkItemPriority,
        WorkItemType,
    };

    fn item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".to_string(),
                business_object_id: "ra-1".to_string(),
                subject_version: "sor-1".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "alice".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1_700_000_000),
        )
        .unwrap()
    }

    #[test]
    fn maps_identity_errors_to_existing_http_kinds() {
        assert!(matches!(
            map_identity_error(CardFundsCommandIdentityError::TaskVersionChanged),
            Error::ConflictError(_)
        ));
        assert!(matches!(
            map_identity_error(CardFundsCommandIdentityError::NotReviewTask),
            Error::BusinessLogicError(_)
        ));
        assert!(matches!(
            map_identity_error(CardFundsCommandIdentityError::NotCurrentOpenOwner),
            Error::Forbidden(_)
        ));
    }

    #[test]
    fn review_and_registration_locks_accept_matching_task() {
        let item = item();
        let account = ReceivableAccountId::new("ra-1");
        assert!(
            lock_review_work_item(&item, "alice", 1, "sor-1", &account, CardFundsReviewType::Opening,)
                .is_ok()
        );
        assert!(lock_registration_work_item(&item, "alice", 1, "sor-1").is_ok());
    }
}
