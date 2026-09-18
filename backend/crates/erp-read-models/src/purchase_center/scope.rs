//! 采购列表与候选的一致授权快照；范围与业务版本跨页携带。

use std::hash::{Hash, Hasher};

use application_core::{AuditActor, FilterOption, FilteredPage};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::PurchaseResolvedScope;
use erp_procurement::dto::purchase_order::{PurchaseOrderListParams, SortDir};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_order::{PurchaseOrderFilter, PurchaseOrderRow};
use persistence_core::Transactional;
use serde::Serialize;
use validator::Validate;

use super::PurchaseOrderReadService;
use super::dto::PurchaseOrderListItemView;
use super::repository::{PurchaseOrderListFacts, load_purchase_order_list_page};
use crate::{Error, Result};

/// 查询直接复用领域 DTO，scope_version 与原有数值参数均在 URL 边界解码。
pub type PurchaseListParams = PurchaseOrderListParams;

/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct PurchaseListView {
    /// 分页结果、负责人候选与归属口径。
    #[serde(flatten)]
    pub data: FilteredPage<PurchaseOrderListItemView>,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`；有规则但对象为空时不设置。
    pub empty_reason: Option<&'static str>,
    /// 当前采购范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

/// 同一事务内的列表快照，供跨页版本复核。
pub(super) struct PurchaseSnapshot {
    /// 当前页投影行。
    pub page: persistence_core::PageResult<PurchaseOrderRow>,
    /// 当前页关联事实。
    pub facts: PurchaseOrderListFacts,
    /// 页码。
    pub page_no: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 当前范围内的负责人候选。
    pub owner_options: Vec<FilterOption>,
    /// 身份授权上下文。
    pub context: PurchaseResolvedScope,
    /// 授权集合本身为空。
    pub no_scope: bool,
}

impl PurchaseOrderReadService {
    /// 授权、总数、候选与业务身份版本全部在同一个事务读取。
    ///
    /// # 参数
    /// * `params` - 原始查询
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的列表快照。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    ///
    /// # 关键业务约束
    /// 负责人筛选只收窄授权结果；候选不授予改派资格。
    pub(super) async fn list_snapshot(
        &self,
        params: &PurchaseOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseSnapshot> {
        let db = self.db.clone();
        let access = self.access();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, scope) = access.resolve(&actor, "list", executor).await?;
                    params.validate()?;
                    let query = params.normalized()?;
                    let (keyword_sales_order_ids, keyword_supplier_ids) =
                        super::repository::list_facts::keyword_reference_ids(
                            &db,
                            query.q.as_deref(),
                            executor,
                        )
                        .await?;
                    let filter = PurchaseOrderFilter {
                        owner_user_ids: query.owner_user_ids,
                        keyword_sales_order_ids,
                        keyword_supplier_ids,
                        purchase_no: query.q,
                        sales_order_id: query.sales_order_id.map(erp_core::ids::SalesOrderId::new),
                        supplier_id: query.supplier_id.map(erp_core::ids::SupplierAccountId::new),
                        status: query.status,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by: Some(query.paging.sort_by.to_string()),
                        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
                    };
                    let (page, facts) = load_purchase_order_list_page(&db, &filter, &scope, executor).await?;
                    let versions = db.purchase_orders().query_versions(&filter, &scope, executor).await?;
                    if versions.len() > 10_000 {
                        return Err(Error::ValidationError(
                            "采购单查询超过上限，请收窄供应商或负责人条件".into(),
                        ));
                    }
                    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
                    versions.hash(&mut fingerprint);
                    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
                    let ids = db.purchase_orders().current_owner_ids(&scope, executor).await?;
                    if ids.len() > 10_000 {
                        return Err(Error::ValidationError("负责人候选超过查询上限".into()));
                    }
                    let owner_options = db.accounts().filter_options(&ids, executor).await?;
                    let no_scope = scope.is_empty();
                    Ok(PurchaseSnapshot {
                        no_scope,
                        page,
                        facts,
                        page_no: filter.page,
                        page_size: filter.page_size,
                        owner_options,
                        context,
                    })
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_version_is_consumed_and_unsupported_owner_alias_is_rejected() {
        let query: PurchaseListParams = serde_json::from_value(
            serde_json::json!({"page": 2, "scope_version": "v1", "owner_user_ids": "a,b"}),
        )
        .unwrap();
        assert_eq!(query.scope_version.as_deref(), Some("v1"));
        assert!(serde_json::from_value::<PurchaseListParams>(serde_json::json!({"owner": "张三"})).is_err());
    }
}
