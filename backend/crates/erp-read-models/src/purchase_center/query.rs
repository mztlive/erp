//! 采购单查询与对象中心视图编排。

mod detail;

use std::collections::HashMap;

use application_core::AuditActor;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::dto::purchase_order::{PageView, PurchaseOrderListParams};
use erp_procurement::entity::purchase_order::{PurchaseOrderRevision, PurchaseOrderSubmission};
use erp_procurement::repository::purchase_order::PurchaseOrderRow;
use persistence_core::NoTransaction;
use validator::Validate;

use super::PurchaseOrderReadService;
use super::dto::{PurchaseOrderCenterView, PurchaseOrderListItemView};
use super::repository::PurchaseOrderListFacts;
use super::scope::PurchaseListView;
use crate::{Error, Result};

impl PurchaseOrderReadService {
    /// 分页查询采购单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）；行金额取自当前
    /// 提交/版本表头汇总（批量取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `ConflictError` - 跨页范围版本缺失或已变化
    /// * `Forbidden` - 没有列表动作权限
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// # 关键业务约束
    /// 列表与总数在同一授权快照完成；后续页必须回传范围版本。
    /// 人员候选不由此接口生产，行负责人姓名仍来自当前页单据事实。
    pub async fn purchase_order_list(
        &self,
        params: &PurchaseOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseListView> {
        let expected = params.scope_version.as_deref();
        crate::support::ensure_deep_page(params.page.unwrap_or(1), expected)?;
        params.validate()?;
        let snapshot = self.list_snapshot(params, actor).await?;
        if expected.is_some_and(|value| value != snapshot.context.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        let items = map_list_items(snapshot.page.items, &snapshot.facts)?;
        let current = self.scope_fingerprint(params, actor).await?;
        if current != snapshot.context.scope_version {
            return Err(crate::support::data_scope_changed("数据范围或业务单据已变化，请刷新"));
        }
        Ok(PurchaseListView {
            scope_version: snapshot.context.scope_version,
            policy_version: snapshot.context.policy_version,
            organization_version: snapshot.context.organization_version,
            as_of: snapshot.context.as_of.as_utc().to_rfc3339(),
            empty_reason: snapshot.no_scope.then_some("no_scope"),
            scope_summary: "采购单当前负责人及单据业务组织范围",
            ownership_basis: "current_procurement_owner",
            data: PageView {
                items,
                total: snapshot.page.total,
                page: snapshot.page_no,
                page_size: snapshot.page_size,
            },
        })
    }

    /// 查询采购单对象中心。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `actor` - 已认证操作人；内部无账号上下文时为空
    ///
    /// # 返回
    /// 返回对象中心视图（当前内容按 版本 > 提交 > 草稿 优先级取用）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在或不可见
    /// * `ConflictError` - 组装过程中范围或单据已变化
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情凭证；返回前再次重验范围版本。
    pub async fn purchase_order_detail(
        &self,
        id: &str,
        actor: Option<&AuditActor>,
    ) -> Result<PurchaseOrderCenterView> {
        let mut access_version = None;
        if let Some(actor) = actor {
            let (_, version) = self.access().detail(actor, id).await?;
            access_version = Some(version);
        }
        let view = self.detail_view(id).await?;
        if let (Some(actor), Some(expected)) = (actor, access_version) {
            let (_, current) = self.access().detail(actor, id).await?;
            if current != expected {
                return Err(crate::support::data_scope_changed("数据范围或采购单已变化，请刷新"));
            }
        }
        Ok(view)
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
        let unique = crate::support::dedup_trimmed_nonempty(account_ids.iter().map(String::as_str));
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let accounts = self.db.accounts().list_by_ids(&unique, &mut NoTransaction).await?;
        Ok(accounts.into_iter().map(|account| (account.base.id, account.name)).collect())
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
    if let Some(id) = revision_pointer
        && let Some(revision) = revisions.get(id)
    {
        return (
            revision.gross_amount.to_string(),
            revision.net_amount.to_string(),
            revision.tax_amount.to_string(),
        );
    }
    (String::new(), String::new(), String::new())
}

/// 把当前页投影行映射为列表视图。
///
/// # 参数
/// * `rows` - 已授权的当前页投影；行字段移入返回视图
/// * `facts` - 同一快照下的关联事实
///
/// # 返回
/// 返回列表行视图。
///
/// # 错误
/// 来源销售单号缺失时返回内部错误。
///
/// # 关键业务约束
/// 映射不得改变授权集合；金额取自当前提交或版本指针。
fn map_list_items(
    rows: Vec<PurchaseOrderRow>,
    facts: &PurchaseOrderListFacts,
) -> Result<Vec<PurchaseOrderListItemView>> {
    rows.into_iter()
        .map(|row| -> Result<PurchaseOrderListItemView> {
            let sales_order_id = row.sales_order_id.to_string();
            let sales_order_no = sales_no_for(&sales_order_id, &facts.sales_order_nos)?;
            let supplier_name = supplier_display(row.supplier_id.as_ref(), &facts.supplier_names);
            let totals = list_row_totals(
                row.current_submission_id.as_deref(),
                row.current_revision_id.as_deref(),
                &facts.submissions,
                &facts.revisions,
            );
            let raw_owner = row.owner_user_id.filter(|owner| !owner.trim().is_empty());
            let (owner_user_id, owner_name) = owner_display(raw_owner, &facts.owner_names);
            Ok(PurchaseOrderListItemView {
                id: row.id,
                purchase_no: row.purchase_no,
                sales_order_id,
                sales_order_no,
                supplier_id: row.supplier_id.to_string(),
                supplier_name,
                purchase_type: row.purchase_type,
                fulfillment_responsibility: row.fulfillment_responsibility,
                payment_term_code: row.payment_term_code,
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
                current_submission_id: row.current_submission_id,
                current_revision_id: row.current_revision_id,
                version: row.version,
                created_at: row.created_at,
            })
        })
        .collect()
}

/// 解析供应商展示名.
///
/// # 参数
/// * `supplier_id` - 供应商账号 ID
/// * `names` - 供应商法定名称映射
///
/// # 返回
/// 返回已登记名称；缺失或空白时返回明确的名称缺失提示。
///
/// # 错误
/// 无.
///
/// # 约束
/// 账号标识仅用于定位名称，不能作为供应商名称展示。
pub(super) fn supplier_display(supplier_id: &str, names: &HashMap<String, String>) -> String {
    names
        .get(supplier_id)
        .map(|name| name.trim())
        .filter(|name| !name.is_empty() && *name != supplier_id)
        .unwrap_or("供应商名称不可用")
        .to_owned()
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
    nos.get(sales_order_id).cloned().ok_or_else(|| Error::Internal("采购单关联的销售单不存在".to_string()))
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
mod query_mapping_tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use erp_core::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SalesOrderId, SupplierAccountId};
    use erp_core::money::Amount;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, ProgressStatus, PurchaseOrderStatus,
        PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseReviewStatus, PurchaseType,
        SupplierSnapshot,
    };

    use super::{
        PurchaseOrderListFacts, PurchaseOrderRow, center_content_source, list_row_totals, map_list_items,
        owner_display, sales_no_for, supplier_display,
    };

    /// 构造已授权的采购列表行，用于执行实际视图映射。
    fn list_row(id: &str) -> PurchaseOrderRow {
        PurchaseOrderRow {
            id: id.to_string(),
            purchase_no: format!("PO-{id}"),
            sales_order_id: SalesOrderId::new("so-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            payment_term_code: "NET-30".to_string(),
            created_by: "creator-1".to_string(),
            owner_user_id: Some("buyer-1".to_string()),
            status: PurchaseOrderStatus::Effective,
            review_status: PurchaseReviewStatus::Approved,
            payment_progress: ProgressStatus::Partial,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::Completed,
            current_submission_id: Some("sub-1".to_string()),
            current_revision_id: None,
            version: 2,
            created_at: 1_700_000_000,
        }
    }

    /// 消费投影行后保持授权行序、身份、指针及负责人回退。
    #[test]
    fn list_mapping_preserves_row_order_and_owned_fields() {
        let mut facts = PurchaseOrderListFacts::default();
        facts.sales_order_nos.insert("so-1".to_string(), "SO-1".to_string());
        facts.owner_names.insert("buyer-1".to_string(), "张三".to_string());
        facts.submissions.insert("sub-1".to_string(), submission("sub-1", "10.00"));
        let mut second = list_row("po-1");
        second.owner_user_id = Some("  ".to_string());
        let items = map_list_items(vec![list_row("po-2"), second], &facts).unwrap();
        assert_eq!(items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["po-2", "po-1"]);
        assert_eq!(items[0].purchase_no, "PO-po-2");
        assert_eq!(items[0].payment_term_code, "NET-30");
        assert_eq!(items[0].current_submission_id.as_deref(), Some("sub-1"));
        assert_eq!(items[0].current_revision_id, None);
        assert_eq!(items[0].gross_amount, "10.00");
        assert_eq!(items[0].supplier_name, "供应商名称不可用");
        assert_eq!(items[0].owner_user_id.as_deref(), Some("buyer-1"));
        assert_eq!(items[0].owner_name, "张三");
        assert_eq!((items[0].version, items[0].created_at), (2, 1_700_000_000));
        assert_eq!(items[1].owner_user_id, None);
        assert_eq!(items[1].owner_name, "未指定");
    }

    /// 非空列表关联销售单缺失时必须停止映射。
    #[test]
    fn list_mapping_rejects_missing_sales_order() {
        let error = map_list_items(vec![list_row("po-1")], &PurchaseOrderListFacts::default()).unwrap_err();
        assert!(matches!(error, crate::Error::Internal(_)));
    }

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
                supplier_revision_id: erp_core::ids::SupplierCommercialProfileRevisionId::new("suprev-1"),
                supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).expect("快照合法"),
                payment_term_snapshot: PaymentTermSnapshot::new(
                    "NET-30".to_string(),
                    false,
                    None,
                    None,
                    crate::purchase_center::test_support::payment_term_fact,
                )
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

    /// 供应商缺失、空白或名称被历史标识污染时明确提示，真实名称正常展示。
    #[test]
    fn supplier_display_requires_readable_name() {
        assert_eq!(supplier_display("sup-1", &HashMap::new()), "供应商名称不可用");
        let mut names = HashMap::new();
        names.insert("sup-1".to_string(), "  ".to_string());
        assert_eq!(supplier_display("sup-1", &names), "供应商名称不可用");
        names.insert("sup-1".to_string(), "sup-1".to_string());
        assert_eq!(supplier_display("sup-1", &names), "供应商名称不可用");
        names.insert("sup-1".to_string(), "  供应商甲  ".to_string());
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
        assert_eq!(owner_display(Some("buyer-1".to_string()), &names).1, "张三".to_string());
    }

    /// 内容来源优先级固定为版本大于提交大于草稿.
    #[test]
    fn content_source_priority_is_revision_over_submission_over_draft() {
        assert_eq!(center_content_source(true, Some("SUBMISSION")), "REVISION".to_string());
        assert_eq!(center_content_source(false, Some("SUBMISSION")), "SUBMISSION".to_string());
        assert_eq!(center_content_source(false, None), "DRAFT".to_string());
    }

    /// 空采购页映射保持为空，不要求关联事实存在。
    #[test]
    fn empty_list_mapping_stays_empty() {
        assert!(map_list_items(Vec::new(), &PurchaseOrderListFacts::default()).unwrap().is_empty());
    }
}
