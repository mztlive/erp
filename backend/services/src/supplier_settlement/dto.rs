//! 域 D33 `supplier_settlement` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额使用 `entities::money`
//! 定点类型（serde_json 下自动字符串化）；业务日期使用 `BusinessDate`（`YYYY-MM-DD`）。

use entities::ids::{
    SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementItemId,
    SupplierSettlementStatementId,
};
use entities::money::Amount;
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SettlementStatus, SupplierSettlementStatement,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};
use crate::supplier_fulfillment::dto::{normalize_sort, PageParams};

/// 结算单列表允许的排序字段白名单（Service 层校验，禁止任意字段透传）。
const STATEMENT_SORT_FIELDS: &[&str] = &["created_at", "period_start", "period_end", "confirmed_at"];
/// 结算明细列表允许的排序字段白名单。
const ITEM_SORT_FIELDS: &[&str] = &["created_at", "erp_calculated_amount", "supplier_billed_amount"];
/// 结算差异列表允许的排序字段白名单。
const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at", "difference_amount", "resolved_at"];

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 供应商结算单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementStatementListParams {
    /// 结算单号模糊筛选（字面量、忽略大小写）。
    pub statement_no: Option<String>,
    /// 结算供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 结算状态筛选。
    pub status: Option<SettlementStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`period_start`/`period_end`/`confirmed_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的结算单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatementListQuery {
    /// 结算单号模糊筛选。
    pub statement_no: Option<String>,
    /// 结算供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 结算状态筛选。
    pub status: Option<SettlementStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierSettlementStatementListParams {
    /// 归一化结算单列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<StatementListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, STATEMENT_SORT_FIELDS)?;
        Ok(StatementListQuery {
            statement_no: normalized_text(self.statement_no.as_deref()),
            supplier_id: self.supplier_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商结算单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementView {
    /// 实体主键。
    pub id: String,
    /// ERP 结算单号（创建幂等键）。
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: String,
    /// 结算期间开始（含）。
    pub period_start: String,
    /// 结算期间结束（含）。
    pub period_end: String,
    /// 供应商账单号。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: Amount,
    /// 供应商金额。
    pub supplier_amount: Amount,
    /// 双方金额差异（= 供应商金额 − ERP 金额）。
    pub difference_amount: Amount,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 经办人。
    pub prepared_by: String,
    /// 复核人。
    pub reviewed_by: Option<String>,
    /// 确认时间（秒级时间戳）。
    pub confirmed_at: Option<i64>,
    /// 确认后形成的应付账户。
    pub payable_account_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierSettlementStatement> for SupplierSettlementStatementView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `statement` - 结算单实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(statement: SupplierSettlementStatement) -> Self {
        Self {
            id: statement.base.id,
            statement_no: statement.statement_no,
            supplier_id: statement.supplier_id.to_string(),
            period_start: statement.period_start.to_string(),
            period_end: statement.period_end.to_string(),
            external_bill_no: statement.external_bill_no,
            external_bill_version: statement.external_bill_version,
            erp_amount: statement.erp_amount,
            supplier_amount: statement.supplier_amount,
            difference_amount: statement.difference_amount,
            status: statement.status,
            prepared_by: statement.prepared_by,
            reviewed_by: statement.reviewed_by,
            confirmed_at: statement.confirmed_at.map(|t| t.unix_secs()),
            payable_account_id: statement.payable_account_id.map(|id| id.to_string()),
            version: statement.base.version,
            created_at: statement.base.created_at,
        }
    }
}

/// 结算明细创建请求行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSettlementItemRequest {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 订单结算金额。
    pub order_amount: Amount,
    /// 运费金额。
    pub freight_amount: Amount,
    /// 服务费金额。
    pub service_fee_amount: Amount,
    /// 供应商退款金额。
    pub refund_amount: Amount,
    /// 供应商账单金额。
    pub supplier_billed_amount: Amount,
}

/// 供应商结算单创建请求（`statement_no` 同时是创建幂等键；表头金额由明细派生）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSettlementStatementRequest {
    /// ERP 结算单号（唯一；重复提交返回原结算单，不重复创建）。
    #[validate(custom(function = "non_blank", message = "结算单号不能为空"))]
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: String,
    /// 结算期间结束（含）。
    pub period_end: String,
    /// 供应商账单号（与版本成对出现）。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本（与账单号成对出现）。
    pub external_bill_version: Option<String>,
    /// 结算明细（至少一行）。
    #[validate(length(min = 1, message = "结算明细至少一行"))]
    pub items: Vec<CreateSettlementItemRequest>,
}

/// 结算单状态推进请求（乐观锁：携带期望版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSettlementReviewRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 提交说明（可选，写入动作摘要）。
    pub comment: Option<String>,
}

/// 结算确认请求（乐观锁 + 复核人；确认后形成应付，§8.4 第 6 条）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConfirmSettlementRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 复核人（不得与经办人相同，实体校验）。
    #[validate(custom(function = "non_blank", message = "复核人不能为空"))]
    pub reviewed_by: String,
}

/// 结算单作废请求（乐观锁 + 原因）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoidSettlementRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 作废原因。
    #[validate(custom(function = "non_blank", message = "作废原因不能为空"))]
    pub reason: String,
}

/// 结算明细列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementItemListParams {
    /// 所属结算单筛选。
    pub statement_id: Option<SupplierSettlementStatementId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`erp_calculated_amount`/`supplier_billed_amount`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的结算明细列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettlementItemListQuery {
    /// 所属结算单筛选。
    pub statement_id: Option<SupplierSettlementStatementId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierSettlementItemListParams {
    /// 归一化结算明细列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SettlementItemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, ITEM_SORT_FIELDS)?;
        Ok(SettlementItemListQuery {
            statement_id: self.statement_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商结算明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementItemView {
    /// 实体主键。
    pub id: String,
    /// 所属结算单。
    pub statement_id: String,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: String,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: String,
    /// 订单结算金额。
    pub order_amount: Amount,
    /// 运费金额。
    pub freight_amount: Amount,
    /// 服务费金额。
    pub service_fee_amount: Amount,
    /// 供应商退款金额。
    pub refund_amount: Amount,
    /// ERP 计算金额（= 订单 + 运费 + 服务费 − 退款）。
    pub erp_calculated_amount: Amount,
    /// 供应商账单金额。
    pub supplier_billed_amount: Amount,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 结算差异列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementDifferenceListParams {
    /// 所属结算明细筛选。
    pub statement_item_id: Option<SupplierSettlementItemId>,
    /// 差异状态筛选。
    pub status: Option<SettlementDifferenceStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`difference_amount`/`resolved_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的结算差异列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettlementDifferenceListQuery {
    /// 所属结算明细筛选。
    pub statement_item_id: Option<SupplierSettlementItemId>,
    /// 差异状态筛选。
    pub status: Option<SettlementDifferenceStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierSettlementDifferenceListParams {
    /// 归一化结算差异列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SettlementDifferenceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DIFFERENCE_SORT_FIELDS)?;
        Ok(SettlementDifferenceListQuery {
            statement_item_id: self.statement_item_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商结算差异响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceView {
    /// 实体主键。
    pub id: String,
    /// 所属结算明细。
    pub statement_item_id: String,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额（有符号）。
    pub difference_amount: Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub resolved_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 结算差异处理请求（乐观锁 + 处理结果三元组，实体校验成组约束）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveDifferenceRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 差异结论状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本（`Compensated`/`Closed` 必填）。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub resolved_at: Option<i64>,
}

/// 结算单详情视图（结算单 + 全部明细 + 全部差异）。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierSettlementStatementDetailView {
    /// 结算单头。
    pub statement: SupplierSettlementStatementView,
    /// 结算明细。
    pub items: Vec<SupplierSettlementItemView>,
    /// 结算差异。
    pub differences: Vec<SupplierSettlementDifferenceView>,
}

/// 供应商结算单分页视图（复用 D32 的契约形状）。
pub type SettlementPageView<T> = crate::supplier_fulfillment::dto::PageView<T>;

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, SupplierSettlementDifferenceListParams, SupplierSettlementItemListParams,
        SupplierSettlementStatementListParams,
    };
    use crate::supplier_fulfillment::dto::SortDir;
    use entities::supplier_settlement::SettlementStatus;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" period_start ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "period_start"],
        )
        .unwrap();
        assert_eq!(field, "period_start");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn statement_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = SupplierSettlementStatementListParams {
            statement_no: Some(" ST-2026 ".to_string()),
            supplier_id: None,
            status: Some(SettlementStatus::PendingReview),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.statement_no.as_deref(), Some("ST-2026"));
        assert_eq!(query.status, Some(SettlementStatus::PendingReview));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = SupplierSettlementItemListParams {
            statement_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let params = SupplierSettlementDifferenceListParams {
            statement_item_id: None,
            status: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("difference_amount".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.sort_by, "difference_amount");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }
}
