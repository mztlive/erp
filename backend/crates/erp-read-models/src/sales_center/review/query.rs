//! 读取销售变更事实并组合创建时冻结的审批绑定。

use application_core::{AuditActor, PageView};
use erp_sales::dto::sales_review::{SalesChangeOrderListParams, SalesChangeOrderView};
use erp_sales::entity::sales_review::SalesChangeOrder;
use erp_sales::repository::prelude::*;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_workflow::BpmExt;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::Transactional;
use serde::Serialize;
use validator::Validate;

use super::projection::document_approval_view;
use super::{SalesChangeOrderDetailView, SalesChangeReadService};
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

/// 销售变更列表保持现有分页字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct SalesChangeListView {
    /// 分页结果。
    #[serde(flatten)]
    pub page: PageView<SalesChangeOrderView>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前销售变更范围口径摘要。
    pub scope_summary: &'static str,
}

impl SalesChangeReadService {
    /// 分页查询销售变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法，或匹配版本超过上限
    /// * `ConflictError` - 跨页范围版本缺失或已变化
    /// * `Forbidden` - 没有列表动作权限
    /// * `Internal` - 未注入授权源
    /// 仓储或事务失败会返回对应错误。
    ///
    /// # 关键业务约束
    /// 沿来源销售单当前负责人和业务组织接入；不得另建平行管理入口。
    pub async fn sales_change_order_list(
        &self,
        params: &SalesChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<SalesChangeListView> {
        let expected = params.scope_version.as_deref();
        crate::support::ensure_deep_page(params.page.unwrap_or(1), expected)?;
        params.validate()?;
        let snapshot = self.change_list_snapshot(params, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        let current = self.change_scope_fingerprint(params, actor).await?;
        if current != snapshot.scope_version {
            return Err(crate::support::data_scope_changed("数据范围或业务单据已变化，请刷新"));
        }
        Ok(snapshot)
    }

    /// 查询销售变更单详情。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在或来源销售单不可见
    /// * `ConflictError` - 两次读取之间数据范围或变更单已变化
    /// * `Internal` - 未注入授权源
    /// 仓储或事务失败会返回对应错误。
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情凭证；沿来源销售单 detail 动作重验。
    pub async fn sales_change_order_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        let first = load_authorized_change(&self.db, self.require_rbac()?, actor, id).await?;
        let current = load_authorized_change(&self.db, self.require_rbac()?, actor, id).await?;
        if first.1 != current.1 {
            return Err(crate::support::data_scope_changed("数据范围或销售变更单已变化，请刷新"));
        }
        Ok(first.0)
    }
}

/// 在独立事务中重验来源销售单后装配变更单详情。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
/// * `actor` - 已认证操作人
/// * `id` - 变更单主键
///
/// # 返回
/// 返回视图和范围版本绑定。
///
/// # 错误
/// 变更单不存在或来源销售单不可见时返回 NotFound。
///
/// # 关键业务约束
/// 不泄露越权变更单的存在性。
async fn load_authorized_change(
    db: &mongodb::Database,
    rbac: &erp_identity::SharedRbacService,
    actor: &AuditActor,
    id: &str,
) -> Result<(SalesChangeOrderDetailView, String)> {
    let actor = actor.clone();
    let id = id.to_string();
    let rbac = rbac.clone();
    let db = db.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { load_change_snapshot(&db, &rbac, &actor, &id, executor).await })
        })
        .await
}

/// 在同一执行器先核验来源详情范围，再装配当前变更事实。
///
/// # 参数
/// * `db` - 销售集合数据库
/// * `rbac` - 当前授权源
/// * `actor` - 当前账号
/// * `id` - 原变更单身份
/// * `executor` - 当前读取事务执行器
///
/// # 返回
/// 返回原单视图及来源范围版本。
///
/// # 错误
/// 来源不可见、缺单、审批关系非法或仓储失败时拒绝。
async fn load_change_snapshot(
    db: &mongodb::Database,
    rbac: &erp_identity::SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(SalesChangeOrderDetailView, String)> {
    let change = db
        .sales_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单不存在或无权查看".to_string()))?;
    let access = SalesAccess::new(db.clone(), rbac.clone());
    let (context, scope) = access.resolve(actor, "detail", &[], executor).await?;
    let order = db
        .sales_orders()
        .find_authorized(change.sales_order_id.as_ref(), &scope, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单不存在或无权查看".to_string()))?;
    let binding = match find_approval_binding(db, id, executor).await.map_err(crate::Error::from) {
        Ok(binding) => binding,
        Err(Error::NotFound(_)) => None,
        Err(error) => return Err(error),
    };
    let version =
        format!("{}:{}:{}:{}", context.scope_version, order.base.id, order.base.version, change.base.version);
    let view = runtime_detail_view(db, rbac, actor, change, binding, executor).await?;
    Ok((view, version))
}

/// 使用已授权原单装配定义、真实运行审批与冻结目标身份。
///
/// # 参数
/// * `db` - 销售集合数据库
/// * `rbac` - 当前授权源
/// * `actor` - 当前账号
/// * `change` - 已核验来源可见的变更单
/// * `binding` - 变更创建时冻结的定义
/// * `executor` - 原授权事务执行器
///
/// # 返回
/// 返回完整只读详情。
///
/// # 错误
/// 定义缺失、当前提交来源或审批事实不一致时拒绝。
async fn runtime_detail_view(
    db: &mongodb::Database,
    rbac: &erp_identity::SharedRbacService,
    actor: &AuditActor,
    change: SalesChangeOrder,
    binding: Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<SalesChangeOrderDetailView> {
    let graph = match binding.as_ref() {
        Some(binding) => Some(
            db.bpm_workflow()
                .load_definition_graph(&binding.approval_process_definition_id, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("销售变更绑定的审批定义不存在".to_string()))?,
        ),
        None => None,
    };
    let content_hash = super::submitted_target::submitted_content_hash(db, &change, executor).await?;
    let approval = super::approval_query::load_change_document_approval(
        db,
        rbac,
        &change,
        binding.as_ref(),
        actor,
        executor,
    )
    .await?;
    let mut view = detail_view(change, binding);
    view.submitted_working_copy_content_hash = content_hash;
    view.approval = approval;
    if let (Some(definition), Some(mut graph)) = (view.approval.definition.as_mut(), graph) {
        definition.name = graph.definition.name;
        graph.nodes.sort_by_key(|node| node.display_order);
        definition.nodes = graph
            .nodes
            .into_iter()
            .map(|node| super::dto::DocumentApprovalNodeView { key: node.node_key, name: node.node_name })
            .collect();
    }
    Ok(view)
}

/// 构建详情视图，并附带只读审批结构。
///
/// # 参数
/// * `change_order` - 变更单
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回详情视图。
fn detail_view(
    change_order: SalesChangeOrder,
    binding: Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>,
) -> SalesChangeOrderDetailView {
    SalesChangeOrderDetailView {
        id: change_order.base.id,
        sales_order_id: change_order.sales_order_id.to_string(),
        base_revision_id: change_order.base_revision_id.to_string(),
        change_type: change_order.change_type,
        reason: change_order.reason,
        status: change_order.stable.status(),
        current_submission_id: change_order.current_submission_id.as_ref().map(ToString::to_string),
        target_content_hash: change_order.target_content_hash,
        submitted_working_copy_content_hash: None,
        effective_revision_id: change_order.effective_revision_id.as_ref().map(ToString::to_string),
        version: change_order.base.version,
        created_at: change_order.base.created_at,
        approval: document_approval_view(binding.as_ref(), None, change_order.stable.status()),
    }
}
