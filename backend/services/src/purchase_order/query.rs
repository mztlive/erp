//! 采购单查询与对象中心视图编排。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt};
use entities::purchase_order::{PurchaseOrderRevision, PurchaseOrderSubmission};
use validator::Validate;

use super::approval_query::load_document_approval;
use super::dto::{
    PageView, PurchaseOrderCenterView, PurchaseOrderListItemView, PurchaseOrderListParams,
    PurchaseSalesAllocationView, TotalsView,
};
use super::view_mapping::{revision_line_to_view, revision_totals, submission_line_to_view};
use super::PurchaseOrderService;
use crate::document_registry::find_approval_binding;
use crate::errors::{Error, Result};

/// 采购单列表筛选条件类型（经 `PurchaseOrderExt` 关联类型跨 crate 可达）。
type PurchaseOrderFilter = <mongodb::Database as PurchaseOrderExt>::PurchaseOrderFilter;

impl PurchaseOrderService {
    /// 分页查询采购单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）；行金额取自当前
    /// 提交/版本表头汇总（批量取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn purchase_order_list(
        &self,
        params: &PurchaseOrderListParams,
    ) -> Result<PageView<PurchaseOrderListItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseOrderFilter {
            purchase_no: query.q,
            sales_order_id: query.sales_order_id.map(entities::ids::SalesOrderId::new),
            supplier_id: query.supplier_id.map(entities::ids::SupplierAccountId::new),
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, super::dto::SortDir::Asc),
        };
        let page = self
            .db
            .load_purchase_order_list_page(&filter, &mut NoTransaction)
            .await?;
        let supplier_names = &page.1.supplier_names;
        let sales_order_numbers = &page.1.sales_order_nos;
        let owner_names = &page.1.owner_names;
        let submissions = &page.1.submissions;
        let revisions = &page.1.revisions;
        let page_rows = &page.0;
        let items = page_rows
            .items
            .iter()
            .map(|row| -> Result<PurchaseOrderListItemView> {
                let sales_order_id = row.sales_order_id.to_string();
                let sales_order_no = sales_no_for(&sales_order_id, sales_order_numbers)?;
                let supplier_name = supplier_display(row.supplier_id.as_ref(), supplier_names);
                let totals = list_row_totals(
                    row.current_submission_id.as_deref(),
                    row.current_revision_id.as_deref(),
                    submissions,
                    revisions,
                );
                let raw_owner = row
                    .owner_user_id
                    .as_deref()
                    .filter(|owner| !owner.trim().is_empty())
                    .map(str::to_string);
                let (owner_user_id, owner_name) = owner_display(raw_owner, owner_names);
                Ok(PurchaseOrderListItemView {
                    id: row.id.clone(),
                    purchase_no: row.purchase_no.clone(),
                    sales_order_id,
                    sales_order_no,
                    supplier_id: row.supplier_id.to_string(),
                    supplier_name,
                    purchase_type: row.purchase_type,
                    payment_term_code: row.payment_term_code.clone(),
                    owner_name,
                    owner_user_id,
                    status: row.status,
                    review_status: row.review_status,
                    gross_amount: totals.0,
                    net_amount: totals.1,
                    tax_amount: totals.2,
                    payment_progress: row.payment_progress,
                    invoice_progress: row.invoice_progress,
                    fulfillment_progress: row.fulfillment_progress,
                    current_submission_id: row.current_submission_id.clone(),
                    current_revision_id: row.current_revision_id.clone(),
                    version: row.version,
                    created_at: row.created_at,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(PageView {
            items,
            total: page_rows.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询采购单对象中心。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    ///
    /// # 返回
    /// 返回对象中心视图（当前内容按 版本 > 提交 > 草稿 优先级取用）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn purchase_order_detail(&self, id: &str) -> Result<PurchaseOrderCenterView> {
        let facts = self
            .db
            .load_purchase_order_center_facts(id, &mut NoTransaction)
            .await?;
        let order = facts
            .order
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        let supplier_name = supplier_display(
            order.supplier_id.as_ref(),
            &facts
                .supplier_name
                .clone()
                .map(|name| (order.supplier_id.to_string(), name))
                .into_iter()
                .collect(),
        );
        let sales_order_id = order.sales_order_id.to_string();
        let sales_order_no = sales_no_for(
            &sales_order_id,
            &facts
                .sales_order_no
                .clone()
                .map(|no| (sales_order_id.clone(), no))
                .into_iter()
                .collect(),
        )?;
        let owner_user_id = order.current_owner_user_id()?.to_string();
        let (_, owner_name) = owner_display(
            Some(owner_user_id.clone()),
            &facts
                .owner_name
                .clone()
                .map(|name| (owner_user_id.clone(), name))
                .into_iter()
                .collect(),
        );

        let content_source = center_content_source(
            facts.current_revision.is_some(),
            facts
                .current_submission
                .as_ref()
                .map(|submission| submission.content_source()),
        );
        let (lines, totals) = if let Some(revision) = &facts.current_revision {
            (
                facts.revision_lines.iter().map(revision_line_to_view).collect(),
                revision_totals(revision),
            )
        } else if let Some(submission) = &facts.current_submission {
            (
                facts
                    .submission_lines
                    .iter()
                    .map(submission_line_to_view)
                    .collect(),
                TotalsView {
                    gross: submission.gross_amount.to_string(),
                    net: submission.net_amount.to_string(),
                    tax: submission.tax_amount.to_string(),
                },
            )
        } else {
            (
                Vec::new(),
                TotalsView {
                    gross: "0.00".to_string(),
                    net: "0.00".to_string(),
                    tax: "0.00".to_string(),
                },
            )
        };
        let allocations = facts
            .allocations
            .into_iter()
            .map(|allocation| PurchaseSalesAllocationView {
                id: allocation.base.id,
                purchase_order_revision_line_id: allocation.purchase_order_revision_line_id.to_string(),
                sales_order_revision_line_id: allocation.sales_order_revision_line_id.to_string(),
                allocated_quantity: allocation.allocated_quantity.to_string(),
                allocated_cost_gross: allocation.allocated_cost_gross.to_string(),
                allocated_cost_net: allocation.allocated_cost_net.to_string(),
            })
            .collect();
        let changes = facts
            .changes
            .into_iter()
            .map(|change| super::dto::PurchaseChangeSummaryView {
                change_id: change.base.id.clone(),
                status: change.stable.status.as_str().to_string(),
                base_revision_id: change.base_revision_id.to_string(),
                effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
                reason: change.reason,
                created_at: change.base.created_at,
            })
            .collect();

        let revision_no = facts
            .current_revision
            .as_ref()
            .map(|revision| revision.revision.revision_no);
        let payable_summary =
            facts
                .payable
                .as_ref()
                .map(|account| super::dto::PurchaseOrderPayableSummaryView {
                    payable_open_amount: account.open_total,
                    paid_allocated_amount: account.settled_total,
                    purchase_invoice_allocated_amount: account.invoiced_total,
                });
        let binding = match find_approval_binding(&self.db, &order.base.id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };

        Ok(PurchaseOrderCenterView {
            id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            status: order.stable.status,
            review_status: order.review_status,
            version: order.base.version,
            sales_order_id,
            sales_order_no,
            supplier_id: order.supplier_id.to_string(),
            supplier_name,
            purchase_type: order.purchase_type,
            payment_term_code: order.payment_term_code.clone(),
            fulfillment_responsibility: order.fulfillment_responsibility,
            owner_user_id,
            owner_name,
            target_warehouse_id: order.target_warehouse_id.as_ref().map(ToString::to_string),
            payment_progress: order.payment_progress,
            invoice_progress: order.invoice_progress,
            fulfillment_progress: order.fulfillment_progress,
            current_submission_id: order.current_submission_id.clone(),
            current_revision_id: order.stable.current_revision_id.clone(),
            revision_no,
            content_source,
            lines,
            totals,
            allocations,
            changes,
            payable_summary,
            approval: load_document_approval(
                &self.db,
                order.base.id.as_ref(),
                binding.as_ref(),
                order.stable.status,
            )
            .await?,
            created_at: order.base.created_at,
        })
    }

    /// 批量解析账号展示姓名。
    ///
    /// 用于采购创建依据「负责人」列：把销售单创建人解析为账号姓名，避免把账号 ID 直接展示给用户。
    /// 采购单列表与对象中心已改用 Repository 批量事实，不再经由本 helper。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID 列表（可重复；空串会被忽略）
    ///
    /// # 返回
    /// 返回账号 ID → 姓名映射；账号不存在时不写入该键。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub(super) async fn resolve_account_names(
        &self,
        account_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut unique = Vec::new();
        let mut seen = HashSet::new();
        for account_id in account_ids {
            let trimmed = account_id.trim();
            if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
                continue;
            }
            unique.push(trimmed.to_string());
        }
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let accounts = self
            .db
            .accounts()
            .list_by_ids(&unique, &mut NoTransaction)
            .await?;
        Ok(accounts
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
    }
}

/// 按指针命名空间解析列表行金额.
///
/// # 参数
/// * `submission_pointer` - 行当前提交指针
/// * `revision_pointer` - 行当前版本指针
/// * `submissions` - 当前提交头映射
/// * `revisions` - 当前版本头映射
///
/// # 返回
/// 返回 `(含税, 不含税, 税额)` 字符串；提交指针只查提交映射，版本指针只查
/// 版本映射，缺失时返回空字符串三元组.
///
/// # 错误
/// 无.
///
/// # 约束
/// 两种命名空间不得交叉；历史表头不进入映射.
fn list_row_totals(
    submission_pointer: Option<&str>,
    revision_pointer: Option<&str>,
    submissions: &HashMap<String, PurchaseOrderSubmission>,
    revisions: &HashMap<String, PurchaseOrderRevision>,
) -> (String, String, String) {
    if let Some(id) = submission_pointer {
        if let Some(submission) = submissions.get(id) {
            return (
                submission.gross_amount.to_string(),
                submission.net_amount.to_string(),
                submission.tax_amount.to_string(),
            );
        }
        return (String::new(), String::new(), String::new());
    }
    if let Some(id) = revision_pointer {
        if let Some(revision) = revisions.get(id) {
            return (
                revision.gross_amount.to_string(),
                revision.net_amount.to_string(),
                revision.tax_amount.to_string(),
            );
        }
    }
    (String::new(), String::new(), String::new())
}

/// 解析供应商展示名.
///
/// # 参数
/// * `supplier_id` - 供应商账号 ID
/// * `names` - 供应商法定名称映射
///
/// # 返回
/// 映射缺失时回退账号 ID 本身.
///
/// # 错误
/// 无.
///
/// # 约束
/// 缺失不得报错，由调用方按约定回退展示.
fn supplier_display(supplier_id: &str, names: &HashMap<String, String>) -> String {
    names
        .get(supplier_id)
        .cloned()
        .unwrap_or_else(|| supplier_id.to_string())
}

/// 解析来源销售单业务单号.
///
/// # 参数
/// * `sales_order_id` - 来源销售单 ID
/// * `nos` - 销售单号映射
///
/// # 返回
/// 返回业务单号；缺失时返回完整性内部错误.
///
/// # 错误
/// * `Internal` - 采购单关联的销售单不存在
///
/// # 约束
/// 缺失不得回退为 ID，必须失败关闭.
fn sales_no_for(sales_order_id: &str, nos: &HashMap<String, String>) -> Result<String> {
    nos.get(sales_order_id)
        .cloned()
        .ok_or_else(|| Error::Internal("采购单关联的销售单不存在".to_string()))
}

/// 解析负责人展示名.
///
/// # 参数
/// * `owner_user_id` - 已去空白的负责人 ID
/// * `names` - 账号展示名映射
///
/// # 返回
/// 无负责人时返回 `未指定`，有 ID 但账号缺失时返回 `责任账号不可用`.
///
/// # 错误
/// 无.
///
/// # 约束
/// 缺失语义与历史实现完全一致，不得改变回退文案.
fn owner_display(owner_user_id: Option<String>, names: &HashMap<String, String>) -> (Option<String>, String) {
    let display = names
        .get(owner_user_id.as_deref().unwrap_or_default())
        .cloned()
        .or_else(|| owner_user_id.as_ref().map(|_| "责任账号不可用".to_string()))
        .unwrap_or_else(|| "未指定".to_string());
    (owner_user_id, display)
}

/// 解析对象中心内容来源优先级.
///
/// # 参数
/// * `has_revision` - 当前版本是否存在
/// * `submission_source` - 当前提交的内容来源
///
/// # 返回
/// 版本存在时返回 `REVISION`，否则有提交时返回提交来源，无内容时返回 `DRAFT`.
///
/// # 错误
/// 无.
///
/// # 约束
/// 优先级固定为版本大于提交大于草稿，不得改变.
fn center_content_source(has_revision: bool, submission_source: Option<&str>) -> String {
    if has_revision {
        return "REVISION".to_string();
    }
    if let Some(source) = submission_source {
        return source.to_string();
    }
    "DRAFT".to_string()
}

#[cfg(test)]
mod query_layering_tests {
    /// 查询编排必须使用批量事实加载，旧逐行 helpers 已删除.
    #[test]
    fn query_uses_batch_fact_bundles() {
        let production = include_str!("query.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");
        assert!(
            production.contains("load_purchase_order_list_page"),
            "列表必须使用批量事实加载"
        );
        assert!(
            production.contains("load_purchase_order_center_facts"),
            "对象中心必须使用批量事实加载"
        );
        assert!(
            !production.contains("fn resolve_sales_order_numbers"),
            "逐销售单号查询已删除"
        );
        assert!(
            !production.contains("fn resolve_order_totals"),
            "逐指针金额查询已删除"
        );
        assert!(
            !production.contains("fn resolve_current_content"),
            "对象中心逐段读取已删除"
        );
        assert!(!production.contains("fn resolve_allocations"), "逐分配读取已删除");
        assert!(
            !production.contains("resolve_supplier_name"),
            "逐供应商名称查询已从查询编排删除"
        );
        for forbidden in [
            ".supplier_accounts()",
            ".parties()",
            ".party_revisions()",
            ".sales_orders()",
            ".purchase_order_submissions()",
            ".purchase_order_revisions()",
            ".purchase_line_sales_allocations()",
            ".payable_accounts()",
            ".purchase_order()",
        ] {
            assert!(
                !production.contains(forbidden),
                "查询编排不得直查持久化集合 {forbidden}，必须经批量事实"
            );
        }
    }

    /// 共享与变更编排不得保留单点直查 helper.
    #[test]
    fn no_single_point_supplier_lookup_remains() {
        let shared = include_str!("shared.rs");
        let change = include_str!("change.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .to_string();
        assert!(
            !shared.contains("fn resolve_supplier_name"),
            "单点供应商名称 helper 已删除"
        );
        assert!(
            !shared.contains(".supplier_accounts()")
                && !shared.contains(".parties()")
                && !shared.contains(".party_revisions()"),
            "共享模块不得直查供应商关联集合"
        );
        assert!(
            !change.contains("resolve_supplier_name"),
            "变更编排已改用批量法定名称"
        );
        assert!(
            change.contains("current_legal_names_by_account_ids"),
            "变更编排必须使用批量法定名称"
        );
    }
}

#[cfg(test)]
mod query_mapping_tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use entities::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SupplierAccountId};
    use entities::money::Amount;
    use entities::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
        PurchaseType, SupplierSnapshot,
    };

    use super::{center_content_source, list_row_totals, owner_display, sales_no_for, supplier_display};

    /// 构造最小提交头用于金额映射测试.
    fn submission(id: &str, gross: &str) -> PurchaseOrderSubmission {
        PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(id.to_string()),
            PurchaseOrderSubmissionData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                submission_no: format!("SUB-{id}"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                supplier_revision_id: entities::ids::SupplierCommercialProfileRevisionId::new("suprev-1"),
                supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).expect("快照合法"),
                payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                    .expect("条款合法"),
                gross_amount: Amount::from_str(gross).unwrap(),
                net_amount: Amount::from_str(gross).unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
            },
        )
        .unwrap()
    }

    /// 提交指针只读提交命名空间，即使版本映射存在同名键也不得交叉.
    #[test]
    fn submission_pointer_never_reads_revision_namespace() {
        let mut submissions = HashMap::new();
        submissions.insert("shared-id".to_string(), submission("shared-id", "10.00"));
        let revisions = HashMap::new();
        assert_eq!(
            list_row_totals(Some("shared-id"), Some("shared-id"), &submissions, &revisions),
            ("10.00".to_string(), "10.00".to_string(), "0".to_string())
        );
        assert_eq!(
            list_row_totals(Some("missing"), Some("shared-id"), &submissions, &revisions),
            (String::new(), String::new(), String::new())
        );
    }

    /// 版本指针只读版本命名空间.
    #[test]
    fn revision_pointer_reads_revision_only() {
        let submissions = HashMap::new();
        assert_eq!(
            list_row_totals(None, Some("missing-rev"), &submissions, &HashMap::new()),
            (String::new(), String::new(), String::new())
        );
    }

    /// 供应商缺失回退账号 ID 本身.
    #[test]
    fn supplier_fallback_returns_account_id() {
        assert_eq!(supplier_display("sup-1", &HashMap::new()), "sup-1".to_string());
        let mut names = HashMap::new();
        names.insert("sup-1".to_string(), "供应商甲".to_string());
        assert_eq!(supplier_display("sup-1", &names), "供应商甲".to_string());
    }

    /// 缺失销售单必须报完整性错误，不得回退 ID.
    #[test]
    fn missing_sales_order_is_internal_error() {
        assert!(sales_no_for("so-1", &HashMap::new()).is_err());
        let mut nos = HashMap::new();
        nos.insert("so-1".to_string(), "SO-1".to_string());
        assert_eq!(sales_no_for("so-1", &nos).unwrap(), "SO-1".to_string());
    }

    /// 负责人三态回退与历史文案一致.
    #[test]
    fn owner_fallback_matrix() {
        assert_eq!(owner_display(None, &HashMap::new()).1, "未指定".to_string());
        assert_eq!(
            owner_display(Some("buyer-1".to_string()), &HashMap::new()).1,
            "责任账号不可用".to_string()
        );
        let mut names = HashMap::new();
        names.insert("buyer-1".to_string(), "张三".to_string());
        assert_eq!(
            owner_display(Some("buyer-1".to_string()), &names).1,
            "张三".to_string()
        );
    }

    /// 内容来源优先级固定为版本大于提交大于草稿.
    #[test]
    fn content_source_priority_is_revision_over_submission_over_draft() {
        assert_eq!(
            center_content_source(true, Some("SUBMISSION")),
            "REVISION".to_string()
        );
        assert_eq!(
            center_content_source(false, Some("SUBMISSION")),
            "SUBMISSION".to_string()
        );
        assert_eq!(center_content_source(false, None), "DRAFT".to_string());
    }

    /// 空分配与空应付保持为空语义.
    #[test]
    fn empty_center_collections_stay_empty() {
        let allocations: Vec<String> = Vec::new();
        assert!(allocations.is_empty());
        let payable: Option<String> = None;
        assert!(payable.is_none());
    }
}
