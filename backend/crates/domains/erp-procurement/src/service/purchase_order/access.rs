//! 采购对象读取与写入范围；当前采购负责人和单据业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::{Permission, internal};
use futures_util::future::BoxFuture;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::purchase_order::PurchaseOrder;
use crate::error::{Error, Result};
use crate::ports::{PurchaseDataScopePort, PurchaseResolvedScope};
use crate::repository::PurchaseOrderExt;
use crate::repository::purchase_order::scope::PurchaseReadScope;

/// 采购对象访问范围；列表、详情、候选、导出、变更／退货和写命令复用同一解析。
#[derive(Clone)]
pub struct PurchaseAccess {
    db: Database,
    scope: Arc<dyn PurchaseDataScopePort>,
}

impl PurchaseAccess {
    /// 绑定采购集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 采购集合所在数据库
    /// * `scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织；不得直接构造身份域 Service。
    pub fn new(db: Database, scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射采购责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `entry` - 中央登记的采购权限常量
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和采购授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配或查询超限时拒绝。
    ///
    /// # 关键业务约束
    /// 当前责任按采购负责人和 `business_org_unit_id` 解释；历史参与须由读取入口另行附加。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        let access = self.scope.resolve(actor, entry, executor).await?;
        let scope = PurchaseReadScope::from_compiled(access.compiled.clone(), access.authorization.clone());
        Ok((access, scope))
    }

    /// 按中央登记的固定内部入口读取采购范围。
    /// # 参数
    /// actor 为已认证身份，entry 为固定权限常量，executor 为原执行器。
    /// # 返回
    /// 固定资格与范围条件下的采购查询凭证。
    /// # 错误
    /// 未登记入口、无权或设施失效时拒绝。
    pub async fn resolve_entry(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        let access = self.scope.resolve_entry(actor, entry, executor).await?;
        let scope = PurchaseReadScope::from_compiled(access.compiled.clone(), access.authorization.clone());
        Ok((access, scope))
    }

    /// 按真实采购对象执行统一授权，不编译列表查询。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `entry` - 中央登记的采购权限常量
    /// * `id` - 采购单主键
    /// * `executor` - 调用方执行器；只读预检可使用非事务执行器
    ///
    /// # 返回
    /// 对象在范围内时返回采购单。
    ///
    /// # 错误
    /// 读取或写动作对不可见对象均返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写入前须在原领域事务内重验；历史参与不授予修改。
    pub async fn require_object(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrder> {
        let db = self.db.clone();
        let id = id.to_owned();
        let target = id.clone();
        authorized_order(self.scope.as_ref(), actor, entry, &target, executor, move |executor| {
            Box::pin(async move { Ok(db.purchase_orders().find_by_id(&id, executor).await?) })
        })
        .await
    }

    /// 按中央登记的固定内部入口读取真实采购对象。
    /// # 参数
    /// actor 为认证身份，entry 为固定权限常量，id 为真实采购身份，executor 为原事务。
    /// # 返回
    /// 完整入口条件覆盖真实对象时返回当前采购单。
    /// # 错误
    /// 入口拒绝或对象不存在返回 NotFound，身份与设施错误原样传播。
    pub async fn require_entry(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrder> {
        self.require_object(actor, entry, id, executor).await
    }

    /// 新单使用即将持久化的显式责任解释创建与提交范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人，通常写入为采购负责人
    /// * `order` - 拟持久化的采购单
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建与提交范围同时覆盖责任事实时成功。
    ///
    /// # 错误
    /// 无创建或提交动作、范围为空或责任不在范围内时拒绝。
    ///
    /// # 关键业务约束
    /// 创建并提交必须由同一角色同时提供两个动作；不得由创建人审计字段兜底。
    pub async fn ensure_create_and_submit(
        &self,
        actor: &AuditActor,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.scope.require_entry(actor, &internal::PURCHASE_CREATE_AND_SUBMIT, order, executor).await
    }

    /// 按固定业务入口检查本次完整来源集合。
    /// # 参数
    /// actor、entry 为当前入口，ids 为完整来源集合，executor 为原事务。
    /// # 返回
    /// 来源全部有权时成功。
    /// # 错误
    /// 未登记或来源无权时拒绝。
    pub async fn require_source(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.scope.require_source(actor, entry, ids, executor).await
    }
}

/// 统一服务先核验记录；业务装载只允许在授权成功后沿用原执行器。
async fn authorized_order<F>(
    scope: &dyn PurchaseDataScopePort,
    actor: &AuditActor,
    entry: &Permission,
    id: &str,
    executor: &mut dyn Executor,
    load: F,
) -> Result<PurchaseOrder>
where
    F: for<'a> FnOnce(&'a mut dyn Executor) -> BoxFuture<'a, Result<Option<PurchaseOrder>>>,
{
    match scope.require_record_entry(actor, entry, id, executor).await {
        Err(Error::Forbidden(_)) => return Err(Error::NotFound("采购单不存在或无权操作".into())),
        Err(error) => return Err(error),
        Ok(()) => {},
    }
    load(executor).await?.ok_or_else(|| Error::NotFound("采购单不存在或无权操作".into()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::common::stable::StableBase;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SupplierAccountId};
    use erp_identity::testing::unavailable_authorization;
    use erp_permissions::{purchase_order, sales_order};
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::PurchaseDataScope;
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, ProgressStatus, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
    };
    use crate::repository::purchase_order::test_database;

    #[derive(Clone, Copy)]
    enum Outcome {
        Allowed,
        Denied,
        Unavailable,
    }

    struct ObjectAccess {
        outcome: Outcome,
        stages: Arc<Mutex<Vec<&'static str>>>,
        candidates: Mutex<Vec<PurchaseOrder>>,
    }

    impl ObjectAccess {
        fn new(outcome: Outcome) -> Self {
            Self { outcome, stages: Arc::default(), candidates: Mutex::default() }
        }
    }

    #[async_trait]
    impl PurchaseDataScopePort for ObjectAccess {
        async fn require_record_entry(
            &self,
            actor: &AuditActor,
            entry: &Permission,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(actor.id(), "actor");
            assert!(
                [purchase_order::DETAIL, purchase_order::SUBMIT, internal::PURCHASE_CHANGE_SOURCE]
                    .contains(entry)
            );
            assert_eq!(id, "purchase-1");
            assert!(executor.boundary().is_some());
            self.stages.lock().unwrap().push("require");
            match self.outcome {
                Outcome::Allowed => Ok(()),
                Outcome::Denied => Err(Error::Forbidden("对象未授权".into())),
                Outcome::Unavailable => Err(Error::Internal("身份设施不可用".into())),
            }
        }

        async fn require_entry(
            &self,
            _: &AuditActor,
            entry: &Permission,
            order: &PurchaseOrder,
            _: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(entry, &internal::PURCHASE_CREATE_AND_SUBMIT);
            self.candidates.lock().unwrap().push(order.clone());
            Ok(())
        }

        async fn resolve(
            &self,
            _: &AuditActor,
            _: &Permission,
            _: &mut dyn Executor,
        ) -> Result<PurchaseResolvedScope> {
            panic!("记录授权不得编译列表查询")
        }
        async fn expand_org_units(
            &self,
            _: &[String],
            _: bool,
            _: &mut dyn Executor,
        ) -> Result<BTreeSet<String>> {
            panic!("记录授权不自行展开组织")
        }
        async fn org_member_ids(
            &self,
            _: &BTreeSet<String>,
            _: Instant,
            _: &mut dyn Executor,
        ) -> Result<Vec<String>> {
            panic!("记录授权不自行读取组织成员")
        }
        async fn own_org(&self, _: &str, _: Instant, _: &mut dyn Executor) -> Result<Option<String>> {
            panic!("记录授权不自行读取主属组织")
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "操作者".into(), AccountKind::Admin)
    }

    fn order() -> PurchaseOrder {
        PurchaseOrder {
            base: BaseModel { id: "purchase-1".into(), version: 7, ..BaseModel::fake() },
            stable: StableBase::new(PurchaseOrderStatus::Draft, "creator"),
            business_org_unit_id: "business-org".into(),
            purchase_no: "PO-1".into(),
            sales_order_id: SalesOrderId::new("sales-1"),
            sales_order_revision_id: SalesOrderRevisionId::new("sales-revision-1"),
            creation_basis_id: "basis-1".into(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".into(),
            fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
            owner_user_id: Some("buyer".into()),
            target_warehouse_id: None,
            review_status: PurchaseReviewStatus::Pending,
            payment_progress: ProgressStatus::None,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::None,
            current_submission_id: None,
            approval_subject_version: 0,
            sourcing_delivery: Default::default(),
        }
    }

    #[tokio::test]
    async fn record_authorization_precedes_business_reading_on_the_original_executor() {
        for entry in [&purchase_order::DETAIL, &purchase_order::SUBMIT, &internal::PURCHASE_CHANGE_SOURCE] {
            let scope = ObjectAccess::new(Outcome::Allowed);
            let stages = scope.stages.clone();
            let mut executor = SnapshotExecutor::default();
            let boundary = executor.boundary().unwrap();
            let loaded =
                authorized_order(&scope, &actor(), entry, "purchase-1", &mut executor, move |executor| {
                    Box::pin(async move {
                        assert!(boundary.is_current(executor));
                        stages.lock().unwrap().push("load");
                        Ok(Some(order()))
                    })
                })
                .await
                .unwrap();
            assert_eq!(loaded, order());
            assert_eq!(*scope.stages.lock().unwrap(), ["require", "load"]);
        }
    }

    #[tokio::test]
    async fn denial_and_identity_failure_stop_before_business_reading() {
        for outcome in [Outcome::Denied, Outcome::Unavailable] {
            let scope = ObjectAccess::new(outcome);
            let error = authorized_order(
                &scope,
                &actor(),
                &purchase_order::SUBMIT,
                "purchase-1",
                &mut SnapshotExecutor::default(),
                |_| panic!("授权失败不得读取业务对象"),
            )
            .await
            .unwrap_err();
            match outcome {
                Outcome::Denied => assert!(matches!(error, Error::NotFound(_))),
                Outcome::Unavailable => {
                    assert!(matches!(error, Error::Internal(message) if message == "身份设施不可用"))
                },
                Outcome::Allowed => unreachable!(),
            }
            assert_eq!(*scope.stages.lock().unwrap(), ["require"]);
        }
    }

    #[tokio::test]
    async fn missing_and_denied_orders_return_the_same_error() {
        let mut executor = SnapshotExecutor::default();
        let missing = authorized_order(
            &ObjectAccess::new(Outcome::Allowed),
            &actor(),
            &purchase_order::DETAIL,
            "purchase-1",
            &mut executor,
            |_| Box::pin(async { Ok(None) }),
        )
        .await
        .unwrap_err();
        let denied = authorized_order(
            &ObjectAccess::new(Outcome::Denied),
            &actor(),
            &purchase_order::DETAIL,
            "purchase-1",
            &mut executor,
            |_| panic!("对象拒绝时不得读取"),
        )
        .await
        .unwrap_err();
        assert!(matches!(missing, Error::NotFound(message) if message == "采购单不存在或无权操作"));
        assert!(matches!(denied, Error::NotFound(message) if message == "采购单不存在或无权操作"));
    }

    #[tokio::test]
    async fn create_and_submit_passes_the_actual_candidate_to_one_fixed_entry() {
        let scope = Arc::new(ObjectAccess::new(Outcome::Allowed));
        let access = PurchaseAccess::new(test_database("purchase_candidate_unit"), scope.clone());
        access.ensure_create_and_submit(&actor(), &order(), &mut ReadExecutor::default()).await.unwrap();
        assert_eq!(*scope.candidates.lock().unwrap(), [order()]);
    }

    #[tokio::test]
    async fn shared_port_uses_the_injected_authorization_before_business_reading() {
        let scope = PurchaseDataScope::shared(
            test_database("purchase_shared_port_unit"),
            unavailable_authorization(),
        );
        let error = authorized_order(
            scope.as_ref(),
            &actor(),
            &purchase_order::DETAIL,
            "purchase-1",
            &mut SnapshotExecutor::default(),
            |_| panic!("共享授权设施失败时不得读取采购单"),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "系统内部错误: 测试授权设施不可用"));
    }
    #[tokio::test]
    async fn shared_port_rejects_a_different_resource_before_identity_or_business_reading() {
        let scope = PurchaseDataScope::shared(
            test_database("purchase_wrong_resource_unit"),
            unavailable_authorization(),
        );
        let mut executor = SnapshotExecutor::default();
        let entry = &sales_order::DETAIL;
        let record = scope.require_record_entry(&actor(), entry, "purchase-1", &mut executor).await;
        let query = scope.resolve(&actor(), entry, &mut executor).await;
        let candidate = scope.require_entry(&actor(), entry, &order(), &mut executor).await;
        for result in [record, query.map(|_| ()), candidate] {
            assert!(
                matches!(result, Err(Error::ValidationError(message)) if message == "采购端口只接受采购单入口")
            );
        }
    }
}
