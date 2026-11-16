use application_core::AuditActor;
use futures_util::future::BoxFuture;
use persistence_core::Executor;
use validator::Validate;

use super::{SettlementAccess, SettlementAction, SupplierSettlementService};
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::{Error, Result};
impl SupplierSettlementService {
    /// 在原执行器授权后准备作废；已作废回放仍核验同一作废资格。
    ///
    /// # 参数
    /// * `id` - 结算单主键。
    /// * `req` - 作废请求，含期望版本。
    /// * `actor` - 当前真实账号。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回结算单和是否已经作废。`true` 表示进入函数时已作废，本次不再改状态；`false` 表示已在内存中作废，本函数不写回。
    ///
    /// # 错误
    /// 作废资格不满足时返回 `Forbidden`。结算单不存在时返回 `NotFound`。草稿不可编辑时返回 `BusinessLogicError`。版本不一致时返回 `ConflictError`。参数校验、状态迁移或仓储读取失败时返回对应错误。
    pub async fn prepare_void(
        &self,
        id: &str,
        req: &VoidSettlementRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, bool)> {
        prepare_void_with(&self.access(), id, req, actor, executor, |executor| {
            Box::pin(self.load_statement(id, executor))
        })
        .await
    }
}

async fn prepare_void_with<'a>(
    access: &SettlementAccess,
    id: &str,
    req: &VoidSettlementRequest,
    actor: &AuditActor,
    executor: &'a mut dyn Executor,
    load: impl FnOnce(&'a mut dyn Executor) -> BoxFuture<'a, Result<SupplierSettlementStatement>> + Send,
) -> Result<(SupplierSettlementStatement, bool)> {
    req.validate()?;
    access.require_action(actor, SettlementAction::Void, id, executor).await?;
    let mut statement = load(executor).await?;
    if statement.is_voided() {
        return Ok((statement, true));
    }
    if !statement.is_editable() {
        return Err(Error::BusinessLogicError("只有经办人可以作废尚未提交复核的结算草稿".to_string()));
    }
    statement
        .ensure_version(req.version)
        .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
    statement.void_draft()?;
    Ok((statement, false))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{SupplierAccountId, SupplierSettlementStatementId};
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::authorization::{FactValue, ObjectFacts};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::{internal, supplier_settlement};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    fn statement(status: SettlementStatus) -> SupplierSettlementStatement {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement"),
            SupplierSettlementStatementData {
                statement_no: "ST-1".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".into(),
                period_policy_version: "1".into(),
                period_timezone: "Asia/Shanghai".into(),
                external_bill_no: None,
                external_bill_version: None,
                erp_amount: "100".parse().unwrap(),
                supplier_amount: "100".parse().unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1),
                source_snapshot_at: Instant::from_unix_secs(1),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: "cutoff".into(),
                refresh_cutoff_policy_version: "1".into(),
                prepared_by: "preparer".into(),
                business_org_unit_id: "org".into(),
                difference_handler_user_id: String::new(),
            },
        )
        .unwrap();
        statement.base = BaseModel { id: "statement".into(), version: 7, ..BaseModel::fake() };
        statement.status = status;
        statement
    }

    fn actor(id: &str) -> AuditActor {
        AuditActor::new(id.into(), id.into(), AccountKind::Admin)
    }

    fn request(version: u64) -> VoidSettlementRequest {
        VoidSettlementRequest { version, reason: "撤销草稿".into() }
    }

    async fn access(with_permission: bool) -> SettlementAccess {
        let mut permissions = vec![internal::SETTLEMENT_VOID_PREPARED];
        if with_permission {
            permissions.push(supplier_settlement::STATEMENT_REFRESH);
        }
        let authorization = fixed_authorization(
            &permissions,
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![ObjectFacts {
                resource: "supplier_settlement_statement".into(),
                id: "statement".into(),
                version: "7".into(),
                owner_id: FactValue::Known(Some("preparer".into())),
                org_unit_id: FactValue::Known(Some("org".into())),
                ..Default::default()
            }],
            Vec::new(),
        )
        .unwrap();
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        SettlementAccess::new(client.database("settlement_void_unit_test"), authorization)
    }

    #[tokio::test]
    async fn first_void_and_replay_reject_missing_permission_or_wrong_preparer_before_reading() {
        for status in [SettlementStatus::Draft, SettlementStatus::Voided] {
            for (with_permission, actor_id) in [(false, "preparer"), (true, "other")] {
                let reads = AtomicUsize::new(0);
                let result = prepare_void_with(
                    &access(with_permission).await,
                    "statement",
                    &request(7),
                    &actor(actor_id),
                    &mut SnapshotExecutor::default(),
                    |_| {
                        Box::pin(async {
                            reads.fetch_add(1, Ordering::SeqCst);
                            Ok(statement(status))
                        })
                    },
                )
                .await;
                assert!(matches!(result, Err(Error::Forbidden(_))), "{status:?}: {result:?}");
                assert_eq!(reads.load(Ordering::SeqCst), 0);
            }
        }
    }

    #[tokio::test]
    async fn authorized_void_and_stale_version_replay_preserve_state_and_version_contract() {
        let access = access(true).await;
        for (status, version, replay) in
            [(SettlementStatus::Draft, 7, false), (SettlementStatus::Voided, 1, true)]
        {
            let original = statement(status);
            let reads = AtomicUsize::new(0);
            let (prepared, already_voided) = prepare_void_with(
                &access,
                "statement",
                &request(version),
                &actor("preparer"),
                &mut SnapshotExecutor::default(),
                |_| {
                    Box::pin(async {
                        reads.fetch_add(1, Ordering::SeqCst);
                        Ok(original.clone())
                    })
                },
            )
            .await
            .unwrap();
            assert_eq!(already_voided, replay);
            assert_eq!(prepared.status, SettlementStatus::Voided);
            assert_eq!(prepared.base.version, 7);
            assert_eq!(reads.load(Ordering::SeqCst), 1);
            if replay {
                assert_eq!(prepared, original);
            }
        }
    }

    #[tokio::test]
    async fn first_void_retains_version_and_non_editable_state_guards() {
        let access = access(true).await;
        for status in [SettlementStatus::Draft, SettlementStatus::PendingReview, SettlementStatus::Confirmed]
        {
            let result = prepare_void_with(
                &access,
                "statement",
                &request(1),
                &actor("preparer"),
                &mut SnapshotExecutor::default(),
                |_| Box::pin(async { Ok(statement(status)) }),
            )
            .await;
            match status {
                SettlementStatus::Draft => assert!(matches!(result, Err(Error::ConflictError(_)))),
                _ => assert!(matches!(result, Err(Error::BusinessLogicError(_)))),
            }
        }
    }
}
