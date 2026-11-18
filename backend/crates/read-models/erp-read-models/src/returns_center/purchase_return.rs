//! PurchaseReturnOrder 详情与分页视图装配。
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::ids::PurchaseReturnOrderId;
use erp_identity::AccessTarget;
use erp_permissions::{internal, purchase_order as purchase_order_permissions};
use erp_procurement::PurchaseResolvedScope;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_returns::entity::returns::{PurchaseReturnLine, PurchaseReturnOrder};
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
use super::repository::purchase_return as authorized_returns;
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
    /// 授权解析时点。
    pub as_of: String,
    /// 预留空结果原因；完整授权查询当前返回 `None`。
    pub empty_reason: Option<&'static str>,
    /// 当前采购退货范围口径摘要。
    pub scope_summary: &'static str,
}

impl ReturnsReadService {
    /// 依据采购退货创建命令的固定资格读取精确结果，不追加详情菜单资格。
    ///
    /// # 参数
    /// * `id` - 原创建命令生成的采购退货主键。
    /// * `source_id` - 原创建命令已校验的采购单主键，必须与持久化结果一致。
    /// * `actor` - 原命令的真实操作人。
    /// * `executor` - 原命令持有的快照事务执行器。
    ///
    /// # 返回
    /// 返回原事务中的退货主单及全部明细。
    ///
    /// # 错误
    /// 非原快照、创建或来源采购修改资格不足、结果来源不符或读取失败时拒绝。
    pub async fn purchase_return_created_result(
        &self,
        id: &str,
        source_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseReturnOrderView> {
        Self::ensure_command_snapshot(executor)?;
        self.authorization
            .require(actor, &internal::PURCHASE_RETURN_SOURCE, AccessTarget::record(source_id)?, executor)
            .await?;

        let order = self
            .db
            .purchase_return_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购退货单不存在".into()))?;
        if order.purchase_order_id.as_ref() != source_id {
            return Err(Error::ValidationError("采购退货命令结果与原采购来源不一致".into()));
        }
        purchase_return_order_with_lines(&self.db, order, executor).await
    }

    /// 调用此入口时，分页查询采购退货单列表。
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

    /// 读取采购退货单详情（退货单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在或来源采购单不可见
    /// * `ConflictError` - 两次读取之间数据范围或退货单已变化
    ///
    /// 仓储或事务失败会返回对应错误。
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

    // 处理采购退货时，授权、总数与退货单版本全部在同一个事务读取。
    //
    // 返回带范围版本的列表快照。
    //
    // 范围变化、筛选非法或仓储失败时拒绝。
    //
    // 业务边界：
    // 筛选只能收窄授权结果；缺范围保持空集。
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
                    let (mut context, scope, filter) = list_query(&access, &params, &actor, executor).await?;

                    let page = authorized_returns::search(&db, &filter, &scope, executor).await?;
                    let versions = authorized_returns::versions(&db, &filter, &scope, executor).await?;
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

                    scope.validate(executor).await?;
                    Ok(PurchaseReturnListView {
                        page: PageView {
                            items: views,
                            total: page.total,
                            page: filter.page,
                            page_size: filter.page_size,
                        },
                        scope_version: context.scope_version,
                        policy_version: context.policy_version,
                        as_of: context.as_of.as_utc().to_rfc3339(),
                        empty_reason: None,
                        scope_summary: "采购退货单沿来源采购单当前负责人及单据业务组织范围",
                    })
                })
            })
            .await
    }

    // 为采购退货绑定范围版本时，独立重验采购来源授权与全部匹配退货版本，不重复加载页行或明细。
    //
    // 调用参数：
    // * `params` - 与首拍相同的列表参数
    //
    // 返回当前授权来源与完整匹配退货版本构成的范围版本。
    //
    // 权限、筛选、来源或退货查询超限以及仓储读取失败时拒绝。
    //
    // 业务边界：
    // 独立事务重新解析授权与历史参与，并重新生成来源采购单限制。
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
                    let (context, scope, filter) = list_query(&access, &params, &actor, executor).await?;

                    let versions = authorized_returns::versions(&db, &filter, &scope, executor).await?;
                    let version = list_scope_version(&context.scope_version, &versions)?;
                    scope.validate(executor).await?;
                    Ok(version)
                })
            })
            .await
    }

    // 读取采购退货时，在独立事务中重验来源采购单后装配退货详情。
    //
    // 调用参数：
    // * `id` - 退货单主键
    //
    // 返回视图和范围版本绑定。
    //
    // 退货单不存在或来源采购单不可见时返回 NotFound。
    //
    // 业务边界：
    // 不泄露越权退货单的存在性。
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
                Box::pin(async move { purchase_return_detail(&db, &access, &id, &actor, executor).await })
            })
            .await
    }
}

// 调用此入口时，同一快照内先证明实际采购来源，再装载采购退货业务字段和明细。
async fn purchase_return_detail(
    db: &Database,
    access: &PurchaseAccess,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(PurchaseReturnOrderView, String)> {
    let (context, scope) = access.resolve(actor, &purchase_order_permissions::DETAIL, executor).await?;
    let (identity, source_id) = authorized_returns::identity(db, id, &scope, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购退货单不存在或无权查看".into()))?;
    let source = db
        .purchase_orders()
        .find_authorized(&source_id, &scope, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购退货单不存在或无权查看".into()))?;

    let view = purchase_return_order_view(db, identity, executor).await?;
    let version = format!("{}:{}:{}", context.scope_version, source.base.id, source.base.version);
    scope.validate(executor).await?;
    Ok((view, version))
}

// 读取采购退货时，在当前快照解析采购授权来源及相同的列表筛选。
//
// 调用参数：
// * `access` - 来源采购单授权访问器
// * `params` - 原始列表查询参数
// * `executor` - 当前快照执行器
//
// 返回完整授权上下文和列表业务筛选。
//
// 授权、来源查询或筛选归一化失败时拒绝。
async fn list_query(
    access: &PurchaseAccess,
    params: &PurchaseReturnOrderListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(PurchaseResolvedScope, PurchaseReadScope, PurchaseReturnOrderFilter)> {
    let (context, scope) = access.resolve(actor, &purchase_order_permissions::LIST, executor).await?;

    let query = params.normalized()?;
    let filter = PurchaseReturnOrderFilter {
        purchase_return_no: query.purchase_return_no,
        purchase_order_id: query.purchase_order_id,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    };
    Ok((context, scope, filter))
}

// 为采购退货绑定范围版本时，对完整匹配退货身份版本绑定授权版本，保持原哈希协议和上限。
//
// 调用参数：
// * `scope_version` - 本次解析的采购授权版本
// * `versions` - 按稳定 ID 排序的全部匹配采购退货版本
//
// 返回跨页和返回前重验使用的完整版本。
//
// 超过 10000 行时返回原有查询上限错误。
fn list_scope_version(scope_version: &str, versions: &[PurchaseReturnVersion]) -> Result<String> {
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("采购退货查询超过上限，请收窄原采购单条件".into()));
    }
    let mut fingerprint = DefaultHasher::new();
    versions.hash(&mut fingerprint);
    Ok(format!("{scope_version}:{:x}", fingerprint.finish()))
}

// 使用首拍的主单投影装配退货视图，保留当前单明细取得的数组顺序。
//
// 调用参数：
// * `row` - 同一授权快照下当前页主单投影
// * `lines` - 原单 ID 查询按原游标顺序返回的明细
//
// 返回原字段与顺序的完整列表行；无明细时返回空数组。
//
// 失败边界：
// 无；主单与明细查询错误由调用方传播。
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

// 处理采购退货时，沿明细查询结果顺序逐行映射展示字段。
//
// 调用参数：
// * `lines` - 单张退货单的明细事实
//
// 返回不排序、不去重的明细展示数组。
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

// 组合采购退货单视图。
//
// 调用参数：
// * `id` - 退货单 ID
//
// 返回完整退货单视图。
//
// 失败边界：
// * `NotFound` - 退货单不存在
//
// 业务边界：
// 明细必须与主单同一执行器读取。
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
    purchase_return_order_with_lines(db, order, executor).await
}

// 使用已取得的精确主单装配明细，维持调用方的同一执行器。
async fn purchase_return_order_with_lines(
    db: &Database,
    order: PurchaseReturnOrder,
    executor: &mut dyn Executor,
) -> Result<PurchaseReturnOrderView> {
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
    use std::sync::Arc;

    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::common::stable::StableBase;
    use erp_core::ids::{PurchaseOrderRevisionLineId, WarehouseId};
    use erp_core::money::Quantity;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_procurement::PurchaseDataScope;
    use erp_procurement::authorization::PurchaseMapping;
    use erp_returns::entity::returns::{PurchaseReturnStatus, ReturnMode};
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    // 验证采购退货：详情实际读取路径必须先检查固定 DETAIL 资格，列表资格不能触发业务读取。
    #[tokio::test]
    async fn purchase_return_detail_denies_missing_detail_permission_before_loading_facts() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("purchase_return_detail_authorization");
        let authorization = fixed_authorization(
            &[purchase_order_permissions::LIST],
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(
                purchase_order_permissions::DETAIL,
                Arc::new(PurchaseMapping::new("purchase_order").unwrap()),
            )],
        )
        .unwrap();
        let access = PurchaseAccess::new(db.clone(), PurchaseDataScope::shared(db.clone(), authorization));
        let actor = AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin);

        let error = purchase_return_detail(&db, &access, "return", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Forbidden(_)));

        let access = ReturnsReadService::new(db.clone(), unavailable_authorization()).purchase_access();
        let error = purchase_return_detail(&db, &access, "return", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "采购范围端口未接线"));
    }

    // 验证采购退货：原命令回包先核对快照和原创建来源资格，不得先读取退货单或明细。
    #[tokio::test]
    async fn created_result_requires_original_snapshot_and_source_authorization_before_loading() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service =
            ReturnsReadService::new(client.database("purchase_return_result"), unavailable_authorization());
        let actor = AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin);

        let error = service
            .purchase_return_created_result("return", "purchase", &actor, &mut ReadExecutor::default())
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(reason) if reason.contains("请打开原单据核对当前状态，勿重复操作"))
        );

        let error = service
            .purchase_return_created_result("return", "purchase", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }

    // 装配采购退货展示时，生成已授权页行，主单字段必须完整映射。
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

    // 处理采购退货时，生成明确输入顺序的明细，包含有仓库与无仓库情况。
    fn line(id: &str, source: &str, quantity: &str, warehouse: Option<&str>) -> PurchaseReturnLine {
        PurchaseReturnLine {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            purchase_return_order_id: PurchaseReturnOrderId::new("return-1"),
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(source),
            return_quantity: Quantity::from_str(quantity).unwrap(),
            warehouse_id: warehouse.map(WarehouseId::new),
        }
    }

    // 验证采购退货：主单投影映射不得丢字段，明细不得按 ID 排序或去重。
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

    // 验证采购退货：无明细或无客户侧依据时保留空数组及原始可选值。
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

    // 验证采购退货：完整匹配 Vec 的原哈希协议不变，空集合仍绑定来源授权版本。
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

    // 验证采购退货：后续页以外的单据变化、删除或授权变化均改变完整匹配指纹。
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

    // 验证采购退货：恰好上限允许完整哈希，超限整体拒绝并保持原错误文案。
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
