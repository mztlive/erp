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
    /// * `action` - 已注册的采购动作
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
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
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
    /// * `action` - 已注册的采购动作
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
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrder> {
        let db = self.db.clone();
        let id = id.to_owned();
        authorized_order(self.scope.as_ref(), actor, action, executor, move |executor| {
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
        match self.scope.require_record_entry(actor, entry, id, executor).await {
            Err(Error::Forbidden(_)) => return Err(Error::NotFound("采购单不存在或无权操作".into())),
            Err(error) => return Err(error),
            Ok(()) => {},
        }
        self.db
            .purchase_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在或无权操作".into()))
    }

    /// 检查当前可信采购对象；供关联任务使用。
    /// # 参数
    /// actor、action 为原入口，order 为当前业务事实，executor 为原事务。
    /// # 返回
    /// 完整对象授权成立时成功。
    /// # 错误
    /// 身份、权限或对象资格失效时拒绝。
    pub async fn require_loaded(
        &self,
        actor: &AuditActor,
        action: &str,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.scope.require(actor, action, order, executor).await
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

/// 已加载对象的可见性由统一授权决定；不存在与对象拒绝使用相同错误。
async fn authorized_order<F>(
    scope: &dyn PurchaseDataScopePort,
    actor: &AuditActor,
    action: &str,
    executor: &mut dyn Executor,
    load: F,
) -> Result<PurchaseOrder>
where
    F: for<'a> FnOnce(&'a mut dyn Executor) -> BoxFuture<'a, Result<Option<PurchaseOrder>>>,
{
    scope.qualify(actor, action, executor).await?;
    let order = load(executor).await?;
    let order = order.ok_or_else(|| Error::NotFound("采购单不存在或无权操作".into()))?;
    match scope.require(actor, action, &order, executor).await {
        Err(Error::Forbidden(_)) => Err(Error::NotFound("采购单不存在或无权操作".into())),
        Err(error) => Err(error),
        Ok(()) => Ok(order),
    }
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

    use super::*;
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, ProgressStatus, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
    };
    use crate::repository::purchase_order::test_database;

    #[derive(Debug, PartialEq, Eq)]
    struct Call {
        actor: String,
        action: String,
        order: PurchaseOrder,
    }

    enum Outcome {
        Allowed,
        Unqualified,
        Denied,
        Unavailable,
    }

    struct ObjectAccess {
        outcome: Outcome,
        calls: Mutex<Vec<Call>>,
        stages: Arc<Mutex<Vec<&'static str>>>,
    }

    impl ObjectAccess {
        fn new(outcome: Outcome) -> Self {
            Self { outcome, calls: Mutex::new(Vec::new()), stages: Arc::default() }
        }
    }

    #[async_trait]
    impl PurchaseDataScopePort for ObjectAccess {
        async fn qualify(&self, _: &AuditActor, _: &str, executor: &mut dyn Executor) -> Result<()> {
            assert!(executor.session().is_none());
            self.stages.lock().unwrap().push("qualification");
            match self.outcome {
                Outcome::Unqualified => Err(Error::Forbidden("当前角色不具备完整功能资格".into())),
                _ => Ok(()),
            }
        }

        async fn require(
            &self,
            actor: &AuditActor,
            action: &str,
            order: &PurchaseOrder,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert!(executor.session().is_none());
            self.stages.lock().unwrap().push("object");
            self.calls.lock().unwrap().push(Call {
                actor: actor.id().into(),
                action: action.into(),
                order: order.clone(),
            });
            match self.outcome {
                Outcome::Allowed => Ok(()),
                Outcome::Unqualified => panic!("功能资格未通过时不得进入对象授权"),
                Outcome::Denied => Err(Error::Forbidden("对象未授权".into())),
                Outcome::Unavailable => Err(Error::Internal("身份设施不可用".into())),
            }
        }

        async fn require_record_entry(
            &self,
            _: &AuditActor,
            entry: &Permission,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert!(executor.boundary().is_some());
            assert_eq!(entry, &internal::PURCHASE_CHANGE_SOURCE);
            assert_eq!(id, "purchase-1");
            self.stages.lock().unwrap().push("fixed_record");
            Err(Error::Forbidden("固定采购记录入口拒绝".into()))
        }

        async fn require_entry(
            &self,
            actor: &AuditActor,
            entry: &Permission,
            order: &PurchaseOrder,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert!(executor.boundary().is_some());
            self.stages.lock().unwrap().push("fixed_entry");
            self.calls.lock().unwrap().push(Call {
                actor: actor.id().into(),
                action: entry.to_string(),
                order: order.clone(),
            });
            Ok(())
        }

        async fn resolve(
            &self,
            _: &AuditActor,
            _: &str,
            _: &mut dyn Executor,
        ) -> Result<PurchaseResolvedScope> {
            panic!("单对象预读不得编译列表授权")
        }

        async fn expand_org_units(
            &self,
            _: &[String],
            _: bool,
            _: &mut dyn Executor,
        ) -> Result<BTreeSet<String>> {
            panic!("单对象授权不自行展开组织")
        }

        async fn org_member_ids(
            &self,
            _: &BTreeSet<String>,
            _: Instant,
            _: &mut dyn Executor,
        ) -> Result<Vec<String>> {
            panic!("单对象授权不自行读取组织成员")
        }

        async fn own_org(&self, _: &str, _: Instant, _: &mut dyn Executor) -> Result<Option<String>> {
            panic!("单对象授权不自行读取主属组织")
        }
    }

    use persistence_core::ReadExecutor;

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
    async fn object_preflight_uses_real_facts_without_query_compilation() {
        for action in ["detail", "submit", "cancel_approval", "update"] {
            let scope = ObjectAccess::new(Outcome::Allowed);
            let mut executor = ReadExecutor::default();
            let original = order();
            let candidate = original.clone();
            let stages = scope.stages.clone();
            let loaded = authorized_order(&scope, &actor(), action, &mut executor, move |executor| {
                Box::pin(async move {
                    assert!(executor.session().is_none());
                    stages.lock().unwrap().push("load");
                    Ok(Some(candidate))
                })
            })
            .await
            .unwrap();
            assert_eq!(loaded, original);
            assert_eq!(*scope.stages.lock().unwrap(), ["qualification", "load", "object"]);
            assert_eq!(
                *scope.calls.lock().unwrap(),
                [Call { actor: "actor".into(), action: action.into(), order: original }],
            );
        }
    }

    #[tokio::test]
    async fn create_and_submit_uses_one_fixed_entry_with_the_actual_candidate() {
        let db = test_database("purchase_access_unit");
        let scope = Arc::new(ObjectAccess::new(Outcome::Allowed));
        let access = PurchaseAccess::new(db, scope.clone());
        let mut executor = ReadExecutor::default();
        let order = order();
        access.ensure_create_and_submit(&actor(), &order, &mut executor).await.unwrap();
        assert_eq!(*scope.stages.lock().unwrap(), ["fixed_entry"]);
        assert_eq!(
            *scope.calls.lock().unwrap(),
            [Call { actor: "actor".into(), action: internal::PURCHASE_CREATE_AND_SUBMIT.to_string(), order }]
        );
    }

    #[tokio::test]
    async fn fixed_record_entry_denial_stops_before_the_business_object_is_read() {
        let scope = Arc::new(ObjectAccess::new(Outcome::Denied));
        let access = PurchaseAccess::new(test_database("purchase_fixed_entry_unit"), scope.clone());
        let mut executor = ReadExecutor::default();
        assert!(matches!(
            access.require_entry(&actor(), &internal::PURCHASE_CHANGE_SOURCE, "purchase-1", &mut executor).await,
            Err(Error::NotFound(message)) if message == "采购单不存在或无权操作"
        ));
        assert_eq!(*scope.stages.lock().unwrap(), ["fixed_record"]);
        assert!(scope.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_function_qualification_is_forbidden_before_business_reading() {
        let scope = ObjectAccess::new(Outcome::Unqualified);
        let mut executor = ReadExecutor::default();
        assert!(matches!(
            authorized_order(&scope, &actor(), "submit", &mut executor, |_| {
                panic!("功能资格失败不得读取业务对象")
            })
            .await,
            Err(Error::Forbidden(message)) if message == "当前角色不具备完整功能资格",
        ));
        assert_eq!(*scope.stages.lock().unwrap(), ["qualification"]);
        assert!(scope.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_or_denied_orders_keep_the_same_not_found_error() {
        let mut executor = ReadExecutor::default();
        let denied = ObjectAccess::new(Outcome::Denied);
        for candidate in [None, Some(order())] {
            assert!(matches!(
                authorized_order(&denied, &actor(), "submit", &mut executor, move |_| {
                    Box::pin(async move { Ok(candidate) })
                }).await,
                Err(Error::NotFound(message)) if message == "采购单不存在或无权操作",
            ));
        }
        assert_eq!(denied.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn identity_errors_are_preserved_instead_of_becoming_missing_objects() {
        let scope = ObjectAccess::new(Outcome::Unavailable);
        let mut executor = ReadExecutor::default();
        assert!(matches!(
            authorized_order(&scope, &actor(), "submit", &mut executor, |_| {
                Box::pin(async { Ok(Some(order())) })
            }).await,
            Err(Error::Internal(message)) if message == "身份设施不可用",
        ));
    }
}
