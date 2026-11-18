//! 销售列表的一致授权快照；范围与业务版本跨页携带。
//! 负责销售候选由销售人员目录提供，不从销售单集合生成。
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, PageView};
use async_trait::async_trait;
use erp_permissions::sales_order as sales_order_permissions;
use erp_sales::dto::sales_order::SalesOrderListParams;
use erp_sales::repository::sales_order::SalesOrderRow;
use erp_sales::repository::sales_order::scope::SalesVersion;
use erp_sales::service::sales_order::SalesOrderService;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::SalesOrderReadService;
use super::dto::SalesOrderView;
use crate::sales_center::access::{SalesAccess, SalesReadContext};
use crate::{Error, Result};

mod export;

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
    /// 授权解析时点。
    pub as_of: String,
    /// 预留空结果原因；完整授权查询当前返回 `None`。
    pub empty_reason: Option<&'static str>,
    /// 当前销售范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

/// 授权分页及原始交付上下文；展示补齐完成前不得结束该读取阶段。
struct SalesListSnapshot<C> {
    page: PageView<SalesOrderRow>,
    context: C,
    scope_version: String,
    as_of: String,
}

/// 列表用例的三个实际读取阶段；不向各仓储扩散测试专用接口。
#[async_trait]
trait SalesListRead: Sync {
    type Context: Send + Sync;

    /// 在原执行器解析权限、关键词与组织筛选，并读取分页和完整匹配版本。
    async fn load(
        &self,
        params: &SalesListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesListSnapshot<Self::Context>>;

    /// 在原执行器补齐本页金额、任务、账号和客户展示。
    async fn present(
        &self,
        page: PageView<SalesOrderRow>,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SalesOrderView>>;

    /// 所有读取完成后核验原凭证，失败时丢弃完整分页。
    async fn validate(&self, context: &Self::Context, executor: &mut dyn Executor) -> Result<()>;
}

impl SalesOrderReadService {
    /// 在同一事务内完成授权、关键词、列表、全部展示事实与最终交付检查。
    ///
    /// # 参数
    /// params 为原始列表查询；actor 为本次已认证操作人。
    /// # 返回
    /// 返回携带完整匹配版本指纹的列表，不提供负责销售候选。
    /// # 错误
    /// 身份、筛选、范围版本、任何事实读取或交付检查失败时拒绝整页。
    pub(super) async fn list_snapshot(
        &self,
        params: &SalesListParams,
        actor: &AuditActor,
    ) -> Result<SalesListView> {
        let service = Self::new(self.db.clone(), self.authorization.clone());
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { read_list(&service, &params, &actor, executor).await })
            })
            .await
    }
}

#[async_trait]
impl SalesListRead for SalesOrderReadService {
    type Context = SalesReadContext;

    /// 签发根凭证后，全部筛选、分页和版本事实使用原事务。
    async fn load(
        &self,
        params: &SalesListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesListSnapshot<Self::Context>> {
        let access = SalesAccess::new(self.db.clone(), self.authorization.clone());
        let (context, scope) = access.resolve(actor, &sales_order_permissions::LIST, executor).await?;

        let org_ids = requested_business_org_units(&access, params, executor).await?;
        let search = self.keyword_search(params.q.as_deref(), executor).await?;
        let (page, versions) = SalesOrderService::new(self.db.clone())
            .list_rows(params, search, &scope, org_ids, executor)
            .await?;

        let scope_version = list_scope_version(&context.scope_version, &versions);
        let as_of = context.as_of.as_utc().to_rfc3339();
        Ok(SalesListSnapshot { page, context, scope_version, as_of })
    }

    /// 使用原列表执行器读取展示事实，保持分页与行序。
    async fn present(
        &self,
        page: PageView<SalesOrderRow>,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SalesOrderView>> {
        self.list_view(page, executor).await
    }

    /// 交付前检查根凭证的原事务、阶段与有效期。
    async fn validate(&self, context: &Self::Context, executor: &mut dyn Executor) -> Result<()> {
        context.validate(executor).await
    }
}

/// 只有完整页面通过原阶段的最终交付检查，才能构造列表响应。
async fn read_list(
    reader: &impl SalesListRead,
    params: &SalesListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SalesListView> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("销售列表必须使用快照事务".into()));
    }

    let snapshot = reader.load(params, actor, executor).await?;
    if params.scope_version.as_deref().is_some_and(|expected| expected != snapshot.scope_version) {
        return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
    }

    let data = reader.present(snapshot.page, executor).await?;
    reader.validate(&snapshot.context, executor).await?;
    Ok(SalesListView {
        scope_version: snapshot.scope_version,
        as_of: snapshot.as_of,
        empty_reason: None,
        scope_summary: "销售单当前负责人、业务组织及合法单据参与范围",
        ownership_basis: "document_sales_owner",
        data,
    })
}

/// 保留原跨页合同：授权版本与全部匹配销售单身份、版本组成指纹。
fn list_scope_version(base: &str, versions: &[SalesVersion]) -> String {
    let mut fingerprint = DefaultHasher::new();
    versions.hash(&mut fingerprint);
    format!("{base}:{:x}", fingerprint.finish())
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
    executor: &mut dyn Executor,
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
mod list_tests;

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
        use erp_core::money::Amount;
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
                    customer_name: Some("测试客户".to_string()),
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
                    gross_amount: Amount::zero(),
                    net_amount: Amount::zero(),
                    tax_amount: Amount::zero(),
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
            as_of: "2026-09-23T00:00:00Z".to_string(),
            empty_reason: None,
            scope_summary: "销售单当前负责人、业务组织及合法单据参与范围",
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
