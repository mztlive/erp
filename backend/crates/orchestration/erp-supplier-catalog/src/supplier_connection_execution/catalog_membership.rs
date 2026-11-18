//! 完整范围重验、旧成员退役及来源撤回；每批复验在事务外，结果在有界事务应用。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_permissions::supplier_api;
use erp_supply::catalog_membership::repository::{self as memberships, CatalogMembershipExt};
use erp_supply::catalog_membership::{
    CatalogMembershipScan, CatalogMembershipState, CatalogPoolMember, CatalogPoolScope,
};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SupplierCatalogRun};
use erp_supply::ports::connector::common::SourceRevision;
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_support::{BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::SupplierConnectionExecutionProcess;
use super::catalog_scan::{classify, verify_connection};
use super::catalog_supply::apply_source_supply;
use crate::{Error, Result};

impl SupplierConnectionExecutionProcess {
    /// 完整目录页保存之后，对同账号授权池内的旧成员逐批核对。
    /// # 参数
    /// `run` 为本次原目录检查点，`actor` 为原任务操作人。
    /// # 返回
    /// 无来源证明、范围待核实或旧成员核对已完成时结束。
    /// # 错误
    /// 范围重验、归属、租期交接或结果事务失败时停止，不重建缺失事实。
    pub(super) async fn reconcile_catalog_memberships(
        &self,
        run: &SupplierCatalogRun,
        actor: &AuditActor,
    ) -> std::result::Result<(), ClassifiedError> {
        let scan = self
            .db
            .supplier_catalog_membership_scans()
            .find_by_id(&run.base.id, &mut NoTransaction)
            .await
            .map_err(|error| classify(error.into()))?;
        let Some(scan) = scan else { return Ok(()) };
        if matches!(scan.state, CatalogMembershipState::Uncertain | CatalogMembershipState::Reconciled) {
            return Ok(());
        }
        if !scan.can_reconcile() {
            return Err(classify(Error::ConflictError("商品池范围未完整闭合，不能执行缺项核对".into())));
        }

        reconcile(&MembershipReconciliation { process: self, run, scan: &scan, actor }, &scan.scope).await
    }

    async fn apply_membership_batch(
        &self,
        run: &SupplierCatalogRun,
        scan: &CatalogMembershipScan,
        actor: &AuditActor,
    ) -> Result<bool> {
        let (db, run, scan, actor) = (self.db.clone(), run.clone(), scan.clone(), actor.clone());
        let authorization = self.authorization.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&actor, &supplier_api::ADOPTIONS_PREVIEW, AccessTarget::Operation, executor)
                        .await?;

                    let connection = SupplierApiService::new(db.clone())
                        .load_connection(&run.connection_id, executor)
                        .await?;
                    verify_connection(&connection, &run)?;
                    if connection.supplier_id.as_ref() != scan.supplier_id {
                        return Err(Error::ConflictError("目录池扫描供应商归属已变化".into()));
                    }
                    let mut job = db
                        .background_jobs()
                        .find_by_id(&run.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("目录同步任务不存在".into()))?;
                    if job.status != JobStatus::Running {
                        return Err(Error::ConflictError("目录同步任务已停止，不能继续撤回来源".into()));
                    }

                    let members =
                        memberships::reconciliation_candidates(&db, &scan, None, 100, executor).await?;
                    if members.is_empty() {
                        memberships::finish_reconciliation(&db, &scan, executor).await?;
                        return Ok(true);
                    }
                    for member in members {
                        memberships::retire_member(&db, &scan, &member, executor).await?;
                        withdraw_missing_member(&db, &scan, &member, &actor, executor).await?;
                    }

                    job.record_progress(0, 0, 0, Instant::now())?;
                    db.background_jobs().update(&mut job, executor).await?;
                    Ok(false)
                })
            })
            .await
    }

    async fn invalidate_membership_scope(&self, run: &SupplierCatalogRun) -> Result<()> {
        let (db, run) = (self.db.clone(), run.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    memberships::abort_scan(&db, &run.base.id, executor).await?;

                    let mut checkpoint = db
                        .supplier_catalog_runs()
                        .find_by_id(&run.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("目录同步检查点不存在".into()))?;
                    checkpoint.record_skipped(vec![
                        "商品池授权范围在完成前发生变化；已保留观察，未继续撤回商品".into(),
                    ])?;
                    db.supplier_catalog_runs().update(&mut checkpoint, executor).await?;
                    Ok(())
                })
            })
            .await
    }
}

async fn withdraw_missing_member(
    db: &Database,
    scan: &CatalogMembershipScan,
    member: &CatalogPoolMember,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if memberships::has_current_membership(db, scan, &member.sku.spec_id, executor).await? {
        return Ok(());
    }

    let items =
        memberships::catalog_items_for_spec(db, &scan.connection_id, &member.sku.spec_id, executor).await?;
    if items.len() > 1 {
        return Err(Error::ConflictError("订货规格的来源商品分组待核实，不能按缺项撤回".into()));
    }
    let Some(mut item) = items.into_iter().next() else { return Ok(()) };
    if item.supplier_id != scan.supplier_id {
        return Err(Error::ConflictError("目录池成员与来源供应商归属不一致".into()));
    }
    if item.availability == CatalogAvailability::Withdrawn {
        return Ok(());
    }

    let at = scan.completed_at.ok_or_else(|| Error::ConflictError("目录池缺少完整读取时点".into()))?;
    if item.withdraw(
        SourceRevision::EqualityToken(format!("catalog-pool-scan:{}", scan.base.id)),
        at,
        &scan.base.id,
    )? {
        db.supplier_catalog_items().update(&mut item, executor).await?;
        apply_source_supply(db, &item, actor, executor).await?;
    }
    Ok(())
}

trait MembershipReconciliationPort {
    fn stopping(&self) -> bool;
    async fn scope(&self) -> std::result::Result<Option<CatalogPoolScope>, ClassifiedError>;
    async fn invalidate(&self) -> std::result::Result<(), ClassifiedError>;
    async fn apply_batch(&self) -> std::result::Result<bool, ClassifiedError>;
}

async fn reconcile<P: MembershipReconciliationPort>(
    port: &P,
    expected: &CatalogPoolScope,
) -> std::result::Result<(), ClassifiedError> {
    loop {
        if port.stopping() {
            return Err(ClassifiedError {
                class: SupplierFailureClass::TransientFailure,
                code: "CATALOG_SHUTDOWN".into(),
                summary: "商品范围核对已暂停，系统将从已保存的位置继续，请稍后查看进度".into(),
            });
        }

        let scope = port.scope().await?.ok_or_else(|| ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "CATALOG_SCOPE_PROOF_REQUIRED".into(),
            summary: "供应商尚未确认完整商品范围，暂不下架未查到的商品，请核对供应商目录".into(),
        })?;
        if &scope != expected {
            port.invalidate().await?;
            return Ok(());
        }

        if port.apply_batch().await? {
            return Ok(());
        }
    }
}

struct MembershipReconciliation<'a> {
    process: &'a SupplierConnectionExecutionProcess,
    run: &'a SupplierCatalogRun,
    scan: &'a CatalogMembershipScan,
    actor: &'a AuditActor,
}

impl MembershipReconciliationPort for MembershipReconciliation<'_> {
    fn stopping(&self) -> bool {
        self.process.control.stopping()
    }
    async fn scope(&self) -> std::result::Result<Option<CatalogPoolScope>, ClassifiedError> {
        let connection = SupplierApiService::new(self.process.db.clone())
            .load_connection(&self.run.connection_id, &mut NoTransaction)
            .await
            .map_err(|error| classify(error.into()))?;
        verify_connection(&connection, self.run).map_err(classify)?;
        self.process.gateway.catalog_scope(&connection).await
    }
    async fn invalidate(&self) -> std::result::Result<(), ClassifiedError> {
        self.process.invalidate_membership_scope(self.run).await.map_err(classify)
    }
    async fn apply_batch(&self) -> std::result::Result<bool, ClassifiedError> {
        self.process.apply_membership_batch(self.run, self.scan, self.actor).await.map_err(classify)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    struct Port {
        scopes: Mutex<VecDeque<std::result::Result<Option<CatalogPoolScope>, ClassifiedError>>>,
        batches: Mutex<VecDeque<std::result::Result<bool, ClassifiedError>>>,
        calls: Mutex<Vec<&'static str>>,
        stopping: bool,
    }

    impl MembershipReconciliationPort for Port {
        fn stopping(&self) -> bool {
            self.stopping
        }
        async fn scope(&self) -> std::result::Result<Option<CatalogPoolScope>, ClassifiedError> {
            self.calls.lock().unwrap().push("scope");
            self.scopes.lock().unwrap().pop_front().expect("expected scope query")
        }
        async fn invalidate(&self) -> std::result::Result<(), ClassifiedError> {
            self.calls.lock().unwrap().push("invalidate");
            Ok(())
        }
        async fn apply_batch(&self) -> std::result::Result<bool, ClassifiedError> {
            self.calls.lock().unwrap().push("apply");
            self.batches.lock().unwrap().pop_front().expect("expected batch")
        }
    }

    fn scope(pools: &[&str]) -> CatalogPoolScope {
        CatalogPoolScope::new("account".into(), pools.iter().map(|pool| (*pool).into()).collect()).unwrap()
    }
    fn port(
        scopes: Vec<std::result::Result<Option<CatalogPoolScope>, ClassifiedError>>,
        batches: Vec<std::result::Result<bool, ClassifiedError>>,
    ) -> Port {
        Port {
            scopes: Mutex::new(scopes.into()),
            batches: Mutex::new(batches.into()),
            calls: Mutex::new(vec![]),
            stopping: false,
        }
    }
    fn failure() -> ClassifiedError {
        ClassifiedError {
            class: SupplierFailureClass::TransientFailure,
            code: "NETWORK".into(),
            summary: "source read failed".into(),
        }
    }

    #[tokio::test]
    async fn each_negative_batch_rechecks_scope_and_resumes_from_saved_progress() {
        let expected = scope(&["one", "two"]);
        let port =
            port(vec![Ok(Some(expected.clone())), Ok(Some(expected.clone()))], vec![Ok(false), Ok(true)]);
        reconcile(&port, &expected).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), ["scope", "apply", "scope", "apply"]);
    }

    #[tokio::test]
    async fn scope_change_or_missing_proof_never_applies_negative_results() {
        let expected = scope(&["one", "two"]);
        let changed = port(vec![Ok(Some(scope(&["one"])))], vec![]);
        reconcile(&changed, &expected).await.unwrap();
        assert_eq!(*changed.calls.lock().unwrap(), ["scope", "invalidate"]);

        let missing = port(vec![Ok(None)], vec![]);
        assert_eq!(reconcile(&missing, &expected).await.unwrap_err().code, "CATALOG_SCOPE_PROOF_REQUIRED");
        assert_eq!(*missing.calls.lock().unwrap(), ["scope"]);
    }

    #[tokio::test]
    async fn failed_scope_or_batch_stops_the_actual_runner() {
        let expected = scope(&["one"]);
        let unreadable = port(vec![Err(failure())], vec![]);
        assert!(reconcile(&unreadable, &expected).await.is_err());
        assert_eq!(*unreadable.calls.lock().unwrap(), ["scope"]);

        let failed = port(vec![Ok(Some(expected.clone()))], vec![Err(failure())]);
        assert!(reconcile(&failed, &expected).await.is_err());
        assert_eq!(*failed.calls.lock().unwrap(), ["scope", "apply"]);
    }

    #[tokio::test]
    async fn deployment_handoff_keeps_original_scan_for_recovery_without_negative_work() {
        let mut port = port(vec![], vec![]);
        port.stopping = true;
        assert_eq!(reconcile(&port, &scope(&["one"])).await.unwrap_err().code, "CATALOG_SHUTDOWN");
        assert!(port.calls.lock().unwrap().is_empty());
    }
}
