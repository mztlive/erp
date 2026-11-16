use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use application_core::{AuditActor, PageView, SortDir, normalize_sort};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, internal, purchase_order};
use erp_procurement::dto::purchase_order::PurchaseChangeOrderListParams;
use erp_procurement::entity::purchase_order::{PurchaseChangeOrder, PurchaseOrder};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_order::PurchaseChangeSearch;
use erp_procurement::repository::purchase_order::scope::PurchaseVersion;
use erp_workflow::service::document_registry::find_approval_binding;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;
use validator::Validate;

use super::super::PurchaseOrderReadService;
use super::super::access::PurchaseAccess;
use super::super::dto::PurchaseChangeOrderView;
use super::coverage_effects::load_coverage_effects;
use super::mapping::change_list_view;
use crate::{Error, Result};

/// 采购变更列表保持现有分页字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct PurchaseChangeListView {
    /// 分页结果。
    #[serde(flatten)]
    pub page: PageView<PurchaseChangeOrderView>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    /// 授权解析时点。
    pub as_of: String,
    /// 预留空结果原因；完整授权查询当前返回 `None`。
    pub empty_reason: Option<&'static str>,
    /// 当前采购变更范围口径摘要。
    pub scope_summary: &'static str,
}

impl PurchaseOrderReadService {
    /// 分页查询采购变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法
    /// * `ConflictError` - 跨页范围版本缺失或已变化
    /// * `Forbidden` - 没有列表动作权限
    ///
    /// # 关键业务约束
    /// 沿来源采购单当前负责人和业务组织接入；不得另建平行管理入口。
    pub async fn change_order_list(
        &self,
        params: &PurchaseChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeListView> {
        let expected = params.scope_version.as_deref();
        crate::support::ensure_deep_page(params.page.unwrap_or(1), expected)?;
        params.validate()?;
        let snapshot = self.change_list_snapshot(params, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        Ok(snapshot)
    }

    /// 查询采购变更单详情。
    ///
    /// 返回统一只读审批结构；创建后未提交只返回绑定定义。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在或来源采购单不可见
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情凭证；沿来源采购单 detail 动作重验。
    pub async fn change_order_detail(&self, id: &str, actor: &AuditActor) -> Result<PurchaseChangeOrderView> {
        load_authorized_change(&self.db, &self.access(), self.rbac.clone(), actor, id).await
    }

    /// 在原命令资格下重新授权并读取采购变更结果。
    /// # 参数
    /// id、actor 为真实变更与当前账号，permission 为固定提交或作废回包权限，executor 为新快照。
    /// # 返回
    /// 同一快照的完整变更结果，不追加详情菜单资格。
    /// # 错误
    /// 非快照、入口不支持、撤权、来源无权或读取失败时拒绝。
    pub async fn command_change_order_view(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeOrderView> {
        if ![internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ, internal::PURCHASE_CHANGE_VOIDED_RESULT_READ]
            .contains(permission)
            || !executor.boundary().is_some_and(|boundary| boundary.is_snapshot())
        {
            return Err(Error::ValidationError("采购变更回包必须使用固定入口和新快照".into()));
        }
        let entry = if *permission == internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ {
            purchase_order::PURCHASE_CHANGE_SUBMIT
        } else {
            purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID
        };
        self.rbac.require(actor, &entry, AccessTarget::Operation, executor).await?;
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".into()))?;
        let target = change.purchase_order_id.clone();
        self.rbac.require(actor, permission, AccessTarget::record(target.as_ref())?, executor).await?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&target, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
        let view = render_change(&self.db, change, order, executor).await?;
        self.rbac.require(actor, permission, AccessTarget::record(target.as_ref())?, executor).await?;
        Ok(view)
    }

    /// 授权、总数与变更单版本全部在同一个事务读取。
    ///
    /// # 参数
    /// * `params` - 原始查询
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的列表快照。
    ///
    /// # 错误
    /// 范围变化、筛选非法或仓储失败时拒绝。
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果；缺范围保持空集。
    async fn change_list_snapshot(
        &self,
        params: &PurchaseChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeListView> {
        let db = self.db.clone();
        let access = self.access();
        let authorization = self.rbac.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    read_change_list(&db, &access, &authorization, &params, &actor, executor).await
                })
            })
            .await
    }
}

/// 在原事务内读取业务筛选与来源授权交集的列表及版本。
async fn read_change_list(
    db: &Database,
    access: &PurchaseAccess,
    authorization: &AuthorizationService,
    params: &PurchaseChangeOrderListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PurchaseChangeListView> {
    authorization
        .require(actor, &purchase_order::PURCHASE_CHANGE_LIST, AccessTarget::Operation, executor)
        .await?;
    let (mut context, scope) = access.resolve(actor, &purchase_order::LIST, executor).await?;
    let (_, sort_dir) = normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
    let purchase_order_id = normalized_filter(params.purchase_order_id.as_deref());
    let status = normalized_filter(params.status.as_deref());
    let result = db
        .purchase_order()
        .search_change_orders(
            PurchaseChangeSearch {
                purchase_order_id: purchase_order_id.as_deref(),
                status: status.as_deref(),
                scope: &scope,
                page,
                page_size,
                sort_ascending: matches!(sort_dir, SortDir::Asc),
            },
            executor,
        )
        .await?;
    let versions = db
        .purchase_order()
        .query_change_versions(purchase_order_id.as_deref(), status.as_deref(), &scope, executor)
        .await?;
    context.scope_version = change_scope_version(&context.scope_version, &versions)?;
    let views = result.items.into_iter().map(|change| change_list_view(change, None)).collect();
    scope.validate(executor).await?;
    Ok(PurchaseChangeListView {
        page: PageView { items: views, total: result.total, page, page_size },
        scope_version: context.scope_version,
        policy_version: context.policy_version,
        as_of: context.as_of.as_utc().to_rfc3339(),
        empty_reason: None,
        scope_summary: "采购变更单沿来源采购单当前负责人及单据业务组织范围",
    })
}

/// 只对完整业务授权交集的变更版本执行上限和指纹绑定。
fn change_scope_version(scope_version: &str, versions: &[PurchaseVersion]) -> Result<String> {
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("采购变更查询超过上限，请收窄原采购单条件".into()));
    }
    let mut fingerprint = DefaultHasher::new();
    versions.hash(&mut fingerprint);
    Ok(format!("{scope_version}:{:x}", fingerprint.finish()))
}

/// 在独立事务中重验来源采购单后装配变更单详情。
///
/// # 参数
/// * `db` - 数据库
/// * `access` - 采购范围访问器
/// * `actor` - 已认证操作人
/// * `id` - 变更单主键
///
/// # 返回
/// 返回通过完整来源权限检查的变更视图。
///
/// # 错误
/// 变更单不存在或来源采购单不可见时返回 NotFound。
///
/// # 关键业务约束
/// 不泄露越权变更单的存在性。
async fn load_authorized_change(
    db: &mongodb::Database,
    access: &super::super::access::PurchaseAccess,
    authorization: Arc<AuthorizationService>,
    actor: &AuditActor,
    id: &str,
) -> Result<PurchaseChangeOrderView> {
    let actor = actor.clone();
    let id = id.to_string();
    let access = access.clone();
    let db = db.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                load_change_snapshot(&db, &access, &authorization, &actor, &id, executor).await
            })
        })
        .await
}

/// 在同一快照内先检查变更入口，再核验实际采购来源及交付。
async fn load_change_snapshot(
    db: &Database,
    access: &PurchaseAccess,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchaseChangeOrderView> {
    authorization
        .require(actor, &purchase_order::PURCHASE_CHANGE_DETAIL, AccessTarget::Operation, executor)
        .await?;
    let change = db
        .purchase_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在或无权查看".to_string()))?;
    let (_, scope) = access.resolve(actor, &purchase_order::DETAIL, executor).await?;
    let order = db
        .purchase_orders()
        .find_authorized(change.purchase_order_id.as_ref(), &scope, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在或无权查看".to_string()))?;
    let view = render_change(db, change, order, executor).await?;
    scope.validate(executor).await?;
    Ok(view)
}

/// 使用原执行器渲染已取得授权的完整变更事实。
async fn render_change(
    db: &Database,
    change: PurchaseChangeOrder,
    order: PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<PurchaseChangeOrderView> {
    let binding = match find_approval_binding(db, &change.base.id, executor).await.map_err(crate::Error::from)
    {
        Ok(binding) => binding,
        Err(Error::NotFound(_)) => None,
        Err(error) => return Err(error),
    };
    let approval = super::super::approval_query::load_change_document_approval(
        db,
        &change.base.id,
        binding.as_ref(),
        change.stable.status,
        executor,
    )
    .await?;
    let coverage_effects = load_coverage_effects(db, &change, &order, executor).await?;
    let mut view = change_list_view(change, binding);
    view.approval = approval;
    view.coverage_effects = coverage_effects;
    Ok(view)
}

/// 规范化可选列表筛选文本。
///
/// # 参数
/// * `value` - 原始可选筛选值
///
/// # 返回
/// 空白值返回 `None`，否则返回去除首尾空白后的字符串。
fn normalized_filter(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod authorization_tests {
    use std::collections::BTreeSet;

    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::testing::unavailable_authorization;
    use erp_procurement::{PurchaseDataScopePort, PurchaseResolvedScope, Result as ProcurementResult};
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    struct UnusedScope;
    #[async_trait::async_trait]
    impl PurchaseDataScopePort for UnusedScope {
        async fn resolve(
            &self,
            _: &AuditActor,
            _: &Permission,
            _: &mut dyn Executor,
        ) -> ProcurementResult<PurchaseResolvedScope> {
            panic!("入口拒绝后不得解析采购来源")
        }
        async fn expand_org_units(
            &self,
            _: &[String],
            _: bool,
            _: &mut dyn Executor,
        ) -> ProcurementResult<BTreeSet<String>> {
            panic!("入口拒绝后不得读取组织")
        }
        async fn org_member_ids(
            &self,
            _: &BTreeSet<String>,
            _: Instant,
            _: &mut dyn Executor,
        ) -> ProcurementResult<Vec<String>> {
            panic!("入口拒绝后不得读取成员")
        }
        async fn own_org(
            &self,
            _: &str,
            _: Instant,
            _: &mut dyn Executor,
        ) -> ProcurementResult<Option<String>> {
            panic!("入口拒绝后不得读取主属组织")
        }
    }

    async fn database() -> Database {
        Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("purchase_change_entry_test")
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn list_and_detail_stop_at_the_native_entry_gate_before_source_or_business_reads() {
        let db = database().await;
        let authorization = unavailable_authorization();
        let access = PurchaseAccess::new(db.clone(), Arc::new(UnusedScope));
        let params = PurchaseChangeOrderListParams {
            scope_version: None,
            purchase_order_id: None,
            status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let result = read_change_list(
            &db,
            &access,
            &authorization,
            &params,
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(result,Err(Error::Internal(message)) if message == "测试授权设施不可用"));
        let result = load_change_snapshot(
            &db,
            &access,
            &authorization,
            &actor(),
            "change",
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(result,Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn command_reply_rejects_nontransaction_and_unrelated_permissions_before_reading() {
        let reads = PurchaseOrderReadService::new(
            database().await,
            unavailable_authorization(),
            Arc::new(UnusedScope),
        );
        let result = reads
            .command_change_order_view(
                "change",
                &actor(),
                &internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ,
                &mut ReadExecutor::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::ValidationError(_))));
        let result = reads
            .command_change_order_view(
                "change",
                &actor(),
                &purchase_order::DETAIL,
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::ValidationError(_))));
    }

    #[tokio::test]
    async fn command_reply_rechecks_its_original_entry_before_loading_metadata() {
        let reads = PurchaseOrderReadService::new(
            database().await,
            unavailable_authorization(),
            Arc::new(UnusedScope),
        );
        for permission in
            [internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ, internal::PURCHASE_CHANGE_VOIDED_RESULT_READ]
        {
            let result = reads
                .command_change_order_view("change", &actor(), &permission, &mut SnapshotExecutor::default())
                .await;
            assert!(matches!(result,Err(Error::Internal(message)) if message == "测试授权设施不可用"));
        }
    }
}
