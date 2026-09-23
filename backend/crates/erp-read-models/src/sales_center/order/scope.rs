//! 销售列表的一致授权快照；范围与业务版本跨页携带。
//! 负责销售候选由销售人员目录提供，不从销售单集合生成。
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, PageView};
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_sales::dto::sales_order::SalesOrderListParams;
use erp_sales::repository::sales_order::{SalesOrderRow, SalesOrderSearch};
use persistence_core::Transactional;
use serde::Serialize;

use super::SalesOrderReadService;
use super::dto::SalesOrderView;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

/// 查询直接复用领域 DTO，scope_version 与原有数值参数均在 URL 边界解码。
pub type SalesListParams = SalesOrderListParams;

/// 列表响应保持分页、归属口径与授权时点，不传输负责销售候选。
#[derive(Serialize)]
pub struct SalesListView {
    /// 分页结果。
    #[serde(flatten)]
    pub data: PageView<SalesOrderView>,
    /// 当前销售责任口径，不代表组织隔离已生效。
    pub ownership_basis: &'static str,
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
    /// 当前销售范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

pub(super) struct SalesSnapshot {
    pub page: PageView<SalesOrderRow>,
    pub context: AuthorizedDataScope,
    pub no_scope: bool,
}
impl SalesOrderReadService {
    /// 授权、总数与业务身份版本全部在同一个事务读取。
    ///
    /// # 参数
    /// * `params` - 原始列表查询，含组织筛选
    /// * `search` - 跨域关键词解析结果
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回授权快照和范围版本，不含负责销售候选。
    ///
    /// # 错误
    /// 无动作权限、组织筛选非法、未知组织或查询超限时拒绝。
    ///
    /// # 关键业务约束
    /// 组织筛选按单据 `business_org_unit_id` 收窄；缺范围保持空集。
    /// 负责人筛选只收窄授权结果，不从销售单收集人员候选。
    pub(super) async fn list_snapshot(
        &self,
        params: &SalesOrderListParams,
        search: SalesOrderSearch,
        actor: &AuditActor,
    ) -> Result<SalesSnapshot> {
        let db = self.db.clone();
        let rbac = self.require_rbac()?.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = SalesAccess::new(db.clone(), rbac);
                    let (mut context, scope) = access.resolve(&actor, "list", &[], executor).await?;
                    let org_ids = requested_business_org_units(&access, &params, executor).await?;
                    let (page, versions) =
                        erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                            .list_rows(&params, search, &scope, org_ids, executor)
                            .await?;
                    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
                    versions.hash(&mut fingerprint);
                    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
                    let no_scope = scope.is_empty();
                    Ok(SalesSnapshot { no_scope, page, context })
                })
            })
            .await
    }
}

/// 将请求中的组织筛选展开为单据业务组织条件。
///
/// # 参数
/// * `access` - 销售范围访问器
/// * `params` - 原始列表查询
/// * `executor` - 调用方执行器
///
/// # 返回
/// 无组织筛选时返回 `None`；否则返回已展开的组织 ID。
///
/// # 错误
/// 包含下级但未提供组织、未知组织或展开失败时拒绝。
///
/// # 关键业务约束
/// 组织筛选匹配当前 `business_org_unit_id`，不得改写历史归属快照。
async fn requested_business_org_units(
    access: &SalesAccess,
    params: &SalesOrderListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = &params.org_unit_ids else {
        if params.include_descendants == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(None);
    };
    if params.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = access
        .expand_org_units(org_ids.as_slice(), params.include_descendants.unwrap_or(false), executor)
        .await?;
    Ok(Some(expanded.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_version_is_consumed_and_unsupported_owner_alias_is_rejected() {
        let query: SalesListParams = serde_json::from_value(
            serde_json::json!({"page": 2, "scope_version": "v1", "owner_user_ids": "a,b"}),
        )
        .unwrap();
        assert_eq!(query.scope_version.as_deref(), Some("v1"));
        assert!(serde_json::from_value::<SalesListParams>(serde_json::json!({"owner": "张三"})).is_err());
        let org: SalesListParams = serde_json::from_value(serde_json::json!({
            "org_unit_ids": "org-1,org-2",
            "include_descendants": true
        }))
        .unwrap();
        assert_eq!(org.org_unit_ids.unwrap().as_slice(), &["org-1".to_string(), "org-2".to_string()]);
        assert_eq!(org.include_descendants, Some(true));
    }

    /// 列表响应不携带负责销售候选，已授权行仍返回负责销售姓名。
    #[test]
    fn sales_list_omits_owner_candidates_and_keeps_row_owner_name() {
        use application_core::PageView;
        use erp_sales::entity::sales_order::{
            BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress,
            InvoiceProgress, OriginSystem, ReviewStatus,
        };

        use super::super::dto::{SalesOrderStageSummary, SalesOrderView};

        let view = SalesListView {
            data: PageView {
                items: vec![SalesOrderView {
                    id: "so-1".to_string(),
                    order_no: "SO-1".to_string(),
                    business_type: BusinessType::GoodsService,
                    origin_system: OriginSystem::Erp,
                    customer_id: "cust-1".to_string(),
                    contract_id: None,
                    commercial_status: CommercialStatus::Draft,
                    review_status: ReviewStatus::NotSubmitted,
                    fulfillment_progress: FulfillmentProgress::NotStarted,
                    collection_progress: CollectionProgress::NotCollected,
                    invoice_progress: InvoiceProgress::NotInvoiced,
                    close_status: CloseStatus::NotSatisfied,
                    effective_at: None,
                    closed_at: None,
                    version: 1,
                    created_at: 1,
                    updated_at: 1,
                    owner_user_id: "sales-1".to_string(),
                    owner_user_name: Some("张三".to_string()),
                    stage: SalesOrderStageSummary {
                        code: "draft",
                        label: "草稿",
                        tone: "neutral",
                        owner_role: None,
                        owner_user_id: None,
                        owner_user_name: None,
                        due_at: None,
                    },
                }],
                total: 1,
                page: 1,
                page_size: 20,
            },
            ownership_basis: "document_sales_owner",
            scope_version: "scope-v1".to_string(),
            policy_version: 3,
            organization_version: 4,
            as_of: "2026-09-23T00:00:00Z".to_string(),
            empty_reason: None,
            scope_summary: "销售单当前负责人、业务组织及有效协作或参与范围",
        };
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("owner_options").is_none());
        assert_eq!(json["ownership_basis"], "document_sales_owner");
        assert_eq!(json["total"], 1);
        assert_eq!(json["page"], 1);
        assert_eq!(json["page_size"], 20);
        assert_eq!(json["scope_version"], "scope-v1");
        assert_eq!(json["items"][0]["owner_user_id"], "sales-1");
        assert_eq!(json["items"][0]["owner_user_name"], "张三");
    }
}
