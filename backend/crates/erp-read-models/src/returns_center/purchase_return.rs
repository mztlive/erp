//! PurchaseReturnOrder 详情与分页视图装配。
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::ids::PurchaseReturnOrderId;
use erp_procurement::PurchaseResolvedScope;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_returns::entity::returns::PurchaseReturnLine;
use erp_returns::repository::ReturnsExt;
use erp_returns::repository::prelude::*;
use erp_returns::repository::returns::{PurchaseReturnOrderRow, PurchaseReturnVersion};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;
use validator::Validate;

use super::ReturnsReadService;
use super::dto::{
    PageView, PurchaseReturnLineView, PurchaseReturnOrderListParams, PurchaseReturnOrderView, SortDir,
};
use crate::purchase_center::access::PurchaseAccess;
use crate::{Error, Result};

/// 采购退货单列表筛选条件类型。
type PurchaseReturnOrderFilter = <mongodb::Database as ReturnsExt>::PurchaseReturnOrderFilter;

/// 采购退货列表保持现有分页字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct PurchaseReturnListView {
    /// 分页结果。
    #[serde(flatten)]
    pub page: PageView<PurchaseReturnOrderView>,
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
    /// 当前采购退货范围口径摘要。
    pub scope_summary: &'static str,
}

impl ReturnsReadService {
    /// 分页查询采购退货单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图。
    ///
    /// # 错误
    /// 跨页范围变化、筛选非法或仓储失败时拒绝。
    ///
    /// # 关键业务约束
    /// 沿来源采购单当前负责人和业务组织接入；不得另建平行管理入口。
    pub async fn purchase_return_order_list(
        &self,
        params: &PurchaseReturnOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnListView> {
        let expected = params.scope_version.as_deref();
        crate::support::ensure_deep_page(params.page.unwrap_or(1), expected)?;
        params.validate()?;
        let snapshot = self.purchase_return_list_snapshot(params, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        let current = self.purchase_return_scope_fingerprint(params, actor).await?;
        if current != snapshot.scope_version {
            return Err(crate::support::data_scope_changed("数据范围或业务单据已变化，请刷新"));
        }
        Ok(snapshot)
    }

    /// 查询采购退货单详情（退货单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在或来源采购单不可见
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情凭证；沿来源采购单 detail 动作重验。
    pub async fn purchase_return_order_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnOrderView> {
        let first = self.load_authorized_purchase_return(id, actor).await?;
        let current = self.load_authorized_purchase_return(id, actor).await?;
        if first.1 != current.1 {
            return Err(crate::support::data_scope_changed("数据范围或采购退货单已变化，请刷新"));
        }
        Ok(first.0)
    }

    /// 授权、总数与退货单版本全部在同一个事务读取。
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
    async fn purchase_return_list_snapshot(
        &self,
        params: &PurchaseReturnOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnListView> {
        let db = self.db.clone();
        let access = self.purchase_access();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, filter, no_scope) =
                        list_query(&access, &params, &actor, executor).await?;
                    let page =
                        db.purchase_return_orders().search_purchase_return_orders(&filter, executor).await?;
                    let versions =
                        db.purchase_return_orders().query_purchase_return_versions(&filter, executor).await?;
                    context.scope_version = list_scope_version(&context.scope_version, &versions)?;
                    let mut views = Vec::with_capacity(page.items.len());
                    // 明细查询未定义排序；保留原单 ID 条件，避免多 ID 查询计划改变行顺序。
                    for row in page.items {
                        let lines = db
                            .purchase_return_lines()
                            .find_lines_by_orders(&[PurchaseReturnOrderId::new(row.id.clone())], executor)
                            .await?;
                        views.push(purchase_return_row_view(row, lines));
                    }
                    Ok(PurchaseReturnListView {
                        page: PageView {
                            items: views,
                            total: page.total,
                            page: filter.page,
                            page_size: filter.page_size,
                        },
                        scope_version: context.scope_version,
                        policy_version: context.policy_version,
                        organization_version: context.organization_version,
                        as_of: context.as_of.as_utc().to_rfc3339(),
                        empty_reason: no_scope.then_some("no_scope"),
                        scope_summary: "采购退货单沿来源采购单当前负责人及单据业务组织范围",
                    })
                })
            })
            .await
    }

    /// 独立重验采购来源授权与全部匹配退货版本，不重复加载页行或明细。
    ///
    /// # 参数
    /// * `params` - 与首拍相同的列表参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回当前授权来源与完整匹配退货版本构成的范围版本。
    ///
    /// # 错误
    /// 权限、筛选、来源或退货查询超限以及仓储读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 独立事务重新解析授权与历史参与，并重新生成来源采购单限制。
    async fn purchase_return_scope_fingerprint(
        &self,
        params: &PurchaseReturnOrderListParams,
        actor: &AuditActor,
    ) -> Result<String> {
        let db = self.db.clone();
        let access = self.purchase_access();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (context, filter, _) = list_query(&access, &params, &actor, executor).await?;
                    let versions =
                        db.purchase_return_orders().query_purchase_return_versions(&filter, executor).await?;
                    list_scope_version(&context.scope_version, &versions)
                })
            })
            .await
    }

    /// 在独立事务中重验来源采购单后装配退货详情。
    ///
    /// # 参数
    /// * `id` - 退货单主键
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回视图和范围版本绑定。
    ///
    /// # 错误
    /// 退货单不存在或来源采购单不可见时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 不泄露越权退货单的存在性。
    async fn load_authorized_purchase_return(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<(PurchaseReturnOrderView, String)> {
        let db = self.db.clone();
        let access = self.purchase_access();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = db
                        .purchase_return_orders()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购退货单不存在或无权查看".to_string()))?;
                    let (context, scope) = access.resolve(&actor, "detail", executor).await?;
                    let source = db
                        .purchase_orders()
                        .find_authorized(order.purchase_order_id.as_ref(), &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购退货单不存在或无权查看".to_string()))?;
                    let view = purchase_return_order_view(&db, order.base.id.clone(), executor).await?;
                    let version =
                        format!("{}:{}:{}", context.scope_version, source.base.id, source.base.version);
                    Ok((view, version))
                })
            })
            .await
    }
}

/// 在当前快照解析采购授权来源及相同的列表筛选。
///
/// # 参数
/// * `access` - 来源采购单授权访问器
/// * `params` - 原始列表查询参数
/// * `actor` - 已认证操作人
/// * `executor` - 当前快照执行器
///
/// # 返回
/// 返回授权上下文、列表筛选和无有效范围标志。
///
/// # 错误
/// 授权、来源查询或筛选归一化失败时拒绝。
async fn list_query(
    access: &PurchaseAccess,
    params: &PurchaseReturnOrderListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(PurchaseResolvedScope, PurchaseReturnOrderFilter, bool)> {
    let (context, scope) = access.resolve(actor, "list", executor).await?;
    let no_scope = scope.is_empty();
    let authorized = access.authorized_source_ids(&scope, executor).await?;
    let query = params.normalized()?;
    let filter = PurchaseReturnOrderFilter {
        purchase_return_no: query.purchase_return_no,
        purchase_order_id: query.purchase_order_id,
        authorized_purchase_order_ids: authorized,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    };
    Ok((context, filter, no_scope))
}

/// 对完整匹配退货身份版本绑定授权版本，保持原哈希协议和上限。
///
/// # 参数
/// * `scope_version` - 本次解析的采购授权版本
/// * `versions` - 按稳定 ID 排序的全部匹配采购退货版本
///
/// # 返回
/// 返回跨页和返回前重验使用的完整版本。
///
/// # 错误
/// 超过 10000 行时返回原有查询上限错误。
fn list_scope_version(scope_version: &str, versions: &[PurchaseReturnVersion]) -> Result<String> {
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("采购退货查询超过上限，请收窄原采购单条件".into()));
    }
    let mut fingerprint = DefaultHasher::new();
    versions.hash(&mut fingerprint);
    Ok(format!("{scope_version}:{:x}", fingerprint.finish()))
}

/// 使用首拍的主单投影装配退货视图，保留当前单明细读取的数组顺序。
///
/// # 参数
/// * `row` - 同一授权快照下当前页主单投影
/// * `lines` - 原单 ID 查询按原游标顺序返回的明细
///
/// # 返回
/// 返回原字段与顺序的完整列表行；无明细时返回空数组。
///
/// # 错误
/// 无；主单与明细查询错误由调用方传播。
fn purchase_return_row_view(
    row: PurchaseReturnOrderRow,
    lines: Vec<PurchaseReturnLine>,
) -> PurchaseReturnOrderView {
    PurchaseReturnOrderView {
        id: row.id,
        purchase_return_no: row.purchase_return_no,
        purchase_order_id: row.purchase_order_id,
        sales_return_case_id: row.sales_return_case_id,
        return_mode: row.return_mode,
        status: row.stable.status(),
        version: row.version,
        created_at: row.created_at,
        lines: purchase_return_line_views(lines),
    }
}

/// 沿明细查询结果顺序逐行映射展示字段。
///
/// # 参数
/// * `lines` - 单张退货单的明细事实
///
/// # 返回
/// 返回不排序、不去重的明细展示数组。
///
/// # 错误
/// 无。
fn purchase_return_line_views(lines: Vec<PurchaseReturnLine>) -> Vec<PurchaseReturnLineView> {
    lines
        .into_iter()
        .map(|line| PurchaseReturnLineView {
            id: line.base.id,
            purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
            return_quantity: line.return_quantity,
            warehouse_id: line.warehouse_id.map(|id| id.to_string()),
        })
        .collect()
}

/// 装配采购退货单视图。
///
/// # 参数
/// * `db` - 数据库
/// * `id` - 退货单 ID
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回完整退货单视图。
///
/// # 错误
/// * `NotFound` - 退货单不存在
///
/// # 关键业务约束
/// 明细必须与主单同一执行器读取。
async fn purchase_return_order_view(
    db: &Database,
    id: String,
    executor: &mut dyn Executor,
) -> Result<PurchaseReturnOrderView> {
    let order = db
        .purchase_return_orders()
        .find_by_id(&id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购退货单不存在".to_string()))?;
    let lines =
        db.purchase_return_lines().find_lines_by_orders(&[order.base.id.clone().into()], executor).await?;
    Ok(PurchaseReturnOrderView {
        id: order.base.id.clone(),
        purchase_return_no: order.purchase_return_no,
        purchase_order_id: order.purchase_order_id.to_string(),
        sales_return_case_id: order.sales_return_case_id.map(|id| id.to_string()),
        return_mode: order.return_mode,
        status: order.stable.status(),
        version: order.base.version,
        created_at: order.base.created_at,
        lines: purchase_return_line_views(lines),
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::common::stable::StableBase;
    use erp_core::ids::{PurchaseOrderRevisionLineId, WarehouseId};
    use erp_core::money::Quantity;
    use erp_returns::entity::returns::{PurchaseReturnStatus, ReturnMode};

    use super::*;

    /// 构造已授权页行，主单字段必须完整映射。
    fn row() -> PurchaseReturnOrderRow {
        PurchaseReturnOrderRow {
            id: "return-1".into(),
            stable: StableBase::new(PurchaseReturnStatus::PendingExecution, "buyer-1"),
            purchase_return_no: "PR-1".into(),
            purchase_order_id: "po-1".into(),
            sales_return_case_id: Some("case-1".into()),
            return_mode: ReturnMode::CompanyWarehouseToSupplier,
            version: 4,
            created_at: 100,
        }
    }

    /// 构造明确输入顺序的明细，包含有仓库与无仓库情况。
    fn line(id: &str, source: &str, quantity: &str, warehouse: Option<&str>) -> PurchaseReturnLine {
        PurchaseReturnLine {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            purchase_return_order_id: PurchaseReturnOrderId::new("return-1"),
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(source),
            return_quantity: Quantity::from_str(quantity).unwrap(),
            warehouse_id: warehouse.map(WarehouseId::new),
        }
    }

    /// 主单投影映射不得丢字段，明细不得按 ID 排序或去重。
    #[test]
    fn purchase_return_projection_preserves_fields_and_line_order() {
        let view = purchase_return_row_view(
            row(),
            vec![
                line("line-z", "revision-2", "2", Some("warehouse-1")),
                line("line-a", "revision-1", "1", None),
            ],
        );
        assert_eq!(view.id, "return-1");
        assert_eq!(view.purchase_return_no, "PR-1");
        assert_eq!(view.purchase_order_id, "po-1");
        assert_eq!(view.sales_return_case_id.as_deref(), Some("case-1"));
        assert_eq!(view.return_mode, ReturnMode::CompanyWarehouseToSupplier);
        assert_eq!(view.status, PurchaseReturnStatus::PendingExecution);
        assert_eq!(view.version, 4);
        assert_eq!(view.created_at, 100);
        assert_eq!(view.lines.iter().map(|line| line.id.as_str()).collect::<Vec<_>>(), ["line-z", "line-a"]);
        assert_eq!(view.lines[0].purchase_order_revision_line_id, "revision-2");
        assert_eq!(view.lines[0].return_quantity, Quantity::from_str("2").unwrap());
        assert_eq!(view.lines[0].warehouse_id.as_deref(), Some("warehouse-1"));
        assert_eq!(view.lines[1].warehouse_id, None);
    }

    /// 无明细或无客户侧依据时保留空数组及原始可选值。
    #[test]
    fn purchase_return_projection_keeps_empty_lines_and_optional_source() {
        let mut input = row();
        input.sales_return_case_id = None;
        input.return_mode = ReturnMode::DirectToSupplier;
        let view = purchase_return_row_view(input, Vec::new());
        assert!(view.lines.is_empty());
        assert_eq!(view.sales_return_case_id, None);
        assert_eq!(view.return_mode, ReturnMode::DirectToSupplier);
    }

    /// 完整匹配 Vec 的原哈希协议不变，空集合仍绑定来源授权版本。
    #[test]
    fn purchase_return_fingerprint_preserves_full_hash_and_empty_scope() {
        let versions = vec![
            PurchaseReturnVersion { id: "return-1".into(), version: 2 },
            PurchaseReturnVersion { id: "return-2".into(), version: 3 },
        ];
        let mut legacy = DefaultHasher::new();
        versions.hash(&mut legacy);
        assert_eq!(list_scope_version("scope", &versions).unwrap(), format!("scope:{:x}", legacy.finish()));
        assert_ne!(list_scope_version("scope", &[]).unwrap(), list_scope_version("new-scope", &[]).unwrap());
    }

    /// 后续页以外的单据变化、删除或授权变化均改变完整匹配指纹。
    #[test]
    fn purchase_return_fingerprint_detects_versions_membership_and_authorization() {
        let mut versions = vec![
            PurchaseReturnVersion { id: "return-1".into(), version: 2 },
            PurchaseReturnVersion { id: "return-2".into(), version: 3 },
        ];
        let first = list_scope_version("scope", &versions).unwrap();
        versions[1].version += 1;
        assert_ne!(first, list_scope_version("scope", &versions).unwrap());
        versions[1].version -= 1;
        assert_ne!(first, list_scope_version("new-scope", &versions).unwrap());
        versions.pop();
        assert_ne!(first, list_scope_version("scope", &versions).unwrap());
    }

    /// 恰好上限允许完整哈希，超限整体拒绝并保持原错误文案。
    #[test]
    fn purchase_return_fingerprint_rejects_over_limit() {
        let mut versions: Vec<_> = (0..10_000)
            .map(|index| PurchaseReturnVersion { id: format!("return-{index:05}"), version: 1 })
            .collect();
        assert!(list_scope_version("scope", &versions).is_ok());
        versions.push(PurchaseReturnVersion { id: "return-over-limit".into(), version: 1 });
        assert!(matches!(
            list_scope_version("scope", &versions),
            Err(Error::ValidationError(message)) if message == "采购退货查询超过上限，请收窄原采购单条件"
        ));
    }
}
