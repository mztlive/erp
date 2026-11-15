//! 销售变更提交正常重试与提交未知恢复共用的授权、完整请求及冻结事实核验。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{sales_order, sales_review};
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_review::{
    SalesChangeSubmissionCommand, SalesChangeSubmissionFact, SalesChangeSubmissionReceipt,
};
use erp_sales::repository::{SalesCommandExt, SalesReviewExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 先证明当前提交资格，再按固定命令ID回读原提交和原实例，完全不使用current_submission_id。
pub(super) async fn replay_submission(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    command: &SalesChangeSubmissionCommand,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    replay(
        &MongoReplay {
            db,
            access: SalesAccess::new(db.clone(), authorization.clone()),
            authorization,
            actor,
        },
        command,
        executor,
    )
    .await
}

/// 只有本独立回执的唯一索引竞争允许进入只读结果恢复。
pub(super) fn receipt_write_error(error: persistence_core::Error) -> Error {
    if error.duplicate_index_name() == Some("uk_sales_command_receipts_id") {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}

#[async_trait]
trait ReplaySource: Sync {
    async fn authorize(
        &self,
        command: &SalesChangeSubmissionCommand,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn receipt(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeSubmissionReceipt>>;
    async fn submission(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeSubmissionFact>>;
}

struct MongoReplay<'a> {
    db: &'a Database,
    access: SalesAccess,
    authorization: Arc<AuthorizationService>,
    actor: &'a AuditActor,
}
#[async_trait]
impl ReplaySource for MongoReplay<'_> {
    async fn authorize(
        &self,
        command: &SalesChangeSubmissionCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if command.actor_id != self.actor.id() {
            return Err(Error::NotFound("销售变更单不存在或无权操作".into()));
        }
        self.authorization
            .require(self.actor, &sales_review::SALES_CHANGE_DRAFT, AccessTarget::Operation, executor)
            .await?;
        let change = self
            .db
            .sales_change_orders()
            .find_by_id(&command.change_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".into()))?;
        self.access
            .require_object(self.actor, &sales_order::SUBMIT, change.sales_order_id.as_ref(), executor)
            .await?;
        Ok(())
    }
    async fn receipt(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeSubmissionReceipt>> {
        Ok(self.db.sales_change_submission_receipts().find_by_id_including_deleted(id, executor).await?)
    }
    async fn submission(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesChangeSubmissionFact>> {
        Ok(self
            .db
            .sales_change_submissions()
            .find_by_id(id, executor)
            .await?
            .as_ref()
            .map(SalesChangeSubmissionFact::from))
    }
}

/// 所有恢复分支只执行此顺序；拒绝授权时不得读取回执或任何历史目标。
async fn replay(
    source: &impl ReplaySource,
    command: &SalesChangeSubmissionCommand,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    source.authorize(command, executor).await?;
    let Some(receipt) = source.receipt(&command.id, executor).await? else {
        return Ok(None);
    };
    let persisted = source
        .submission(&receipt.submission.submission_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售变更提交回执引用的冻结提交不存在".into()))?;
    receipt.validate(command, &persisted)?;
    Ok(Some(receipt.approval_process_instance_id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use erp_approval_runtime::approval_recovery::recover_approval_command;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use mongodb::error::Error as MongoError;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;

    struct Store {
        allowed: bool,
        receipts: HashMap<String, SalesChangeSubmissionReceipt>,
        submissions: HashMap<String, SalesChangeSubmissionFact>,
        calls: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl ReplaySource for Store {
        async fn authorize(&self, _: &SalesChangeSubmissionCommand, _: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push("authorize".into());
            if !self.allowed {
                return Err(Error::NotFound("销售变更单不存在或无权操作".into()));
            }
            Ok(())
        }
        async fn receipt(
            &self,
            id: &str,
            _: &mut dyn Executor,
        ) -> Result<Option<SalesChangeSubmissionReceipt>> {
            self.calls.lock().unwrap().push(format!("receipt:{id}"));
            Ok(self.receipts.get(id).cloned())
        }
        async fn submission(
            &self,
            id: &str,
            _: &mut dyn Executor,
        ) -> Result<Option<SalesChangeSubmissionFact>> {
            self.calls.lock().unwrap().push(format!("submission:{id}"));
            Ok(self.submissions.get(id).cloned())
        }
    }
    fn record(version: u64, key: &str, no: u32) -> SalesChangeSubmissionReceipt {
        let command =
            SalesChangeSubmissionCommand::new("change", "actor", version, key, Some("原说明")).unwrap();
        let submission = SalesChangeSubmissionFact {
            submission_id: format!("s{no}"),
            change_id: "change".into(),
            submission_no: no,
            source_version: Some(version),
            submitted_by: "actor".into(),
        };
        SalesChangeSubmissionReceipt::new(command, submission, format!("instance{no}"), format!("audit{no}"))
            .unwrap()
    }
    fn store(records: &[SalesChangeSubmissionReceipt]) -> Store {
        Store {
            allowed: true,
            receipts: records.iter().map(|r| (r.command.id.clone(), r.clone())).collect(),
            submissions: records
                .iter()
                .map(|r| (r.submission.submission_id.clone(), r.submission.clone()))
                .collect(),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 正常提交与回执恢复先验证独立入口；资格取证失败时不能触及变更或回执仓储。
    #[tokio::test]
    async fn submission_entry_is_checked_before_loading_change_or_receipt() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("sales_change_entry_test");
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let command = SalesChangeSubmissionCommand::new("change", actor.id(), 1, "submit", None).unwrap();
        let error = replay_submission(
            &db,
            unavailable_authorization(),
            &actor,
            &command,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let error = replay_submission(&db, unavailable_authorization(), &actor, &command, &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
    }

    /// 后续 S2 已存在时原 K1 仍精确恢复 S1，不读 S2、不写任何事实。
    #[tokio::test]
    async fn original_request_replays_first_instance_after_second_submission() {
        let first = record(4, "k1", 1);
        let second = record(7, "k2", 2);
        let source = store(&[first.clone(), second.clone()]);
        let found = replay(&source, &first.command, &mut ReadExecutor::default()).await.unwrap();
        assert_eq!(found.as_deref(), Some("instance1"));
        assert_eq!(
            source.calls.lock().unwrap().as_slice(),
            ["authorize", &format!("receipt:{}", first.command.id), "submission:s1"]
        );
        assert_eq!(source.receipts[&second.command.id], second);
    }

    /// 撞键/提交未知恢复复用本函数，不能把版本5提交当成版本4请求的成功结果。
    #[tokio::test]
    async fn duplicate_or_unknown_recovery_rejects_another_source_version() {
        let winner = record(5, "same-key", 1);
        let original =
            SalesChangeSubmissionCommand::new("change", "actor", 4, "same-key", Some("原说明")).unwrap();
        assert_eq!(winner.command.id, original.id);
        let source = store(std::slice::from_ref(&winner));
        for error in [
            Error::ReceiptDuplicate(persistence_core::Error::DuplicateKey(MongoError::custom("receipt"))),
            Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(MongoError::custom(
                "unknown",
            ))),
        ] {
            let error = recover_approval_command(error, || async {
                replay(&source, &original, &mut ReadExecutor::default()).await
            })
            .await
            .unwrap_err();
            assert!(matches!(error, Error::ConflictError(_)));
        }
        assert_eq!(source.receipts[&original.id], winner);
        assert_eq!(
            replay(&source, &winner.command, &mut ReadExecutor::default()).await.unwrap().as_deref(),
            Some("instance1")
        );
    }

    /// 相同操作号改说明或持久化提交来源版本不符均失败关闭。
    #[tokio::test]
    async fn replay_refuses_changed_explanation_and_mismatched_frozen_fact() {
        let original = record(4, "key", 1);
        let mut source = store(std::slice::from_ref(&original));
        let changed = SalesChangeSubmissionCommand::new("change", "actor", 4, "key", Some("改说明")).unwrap();
        assert!(matches!(
            replay(&source, &changed, &mut ReadExecutor::default()).await,
            Err(Error::ConflictError(_))
        ));
        source.submissions.get_mut("s1").unwrap().source_version = Some(5);
        assert!(matches!(
            replay(&source, &original.command, &mut ReadExecutor::default()).await,
            Err(Error::ConflictError(_))
        ));
    }

    /// 当前授权撤销后即使原回执存在也不能读取或恢复原提交。
    #[tokio::test]
    async fn replay_checks_current_permission_before_receipt_or_submission() {
        let original = record(4, "key", 1);
        let mut source = store(std::slice::from_ref(&original));
        source.allowed = false;
        let unknown = Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(
            MongoError::custom("unknown"),
        ));
        assert!(matches!(
            recover_approval_command(unknown, || async {
                replay(&source, &original.command, &mut ReadExecutor::default()).await
            })
            .await,
            Err(Error::NotFound(_))
        ));
        assert_eq!(source.calls.lock().unwrap().as_slice(), ["authorize"]);
    }

    /// 旧数据没有完整命令回执时不根据当前提交或历史序号猜测成功。
    #[tokio::test]
    async fn missing_command_receipt_is_not_a_successful_replay() {
        let original = record(4, "key", 1);
        let mut source = store(std::slice::from_ref(&original));
        source.receipts.clear();
        assert!(replay(&source, &original.command, &mut ReadExecutor::default()).await.unwrap().is_none());
        assert_eq!(source.calls.lock().unwrap().len(), 2);
    }
}
