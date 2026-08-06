//! 域 D20 `cost` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串。契约来源：`erp-client/features/actual-profit-loss`
//! （W16 实际经营盈亏；利润指标按不含税口径，本域成本金额三元组同时携带）。

use entities::common::time::Instant;
use entities::cost::{CostBasis, CostScope, CostStage, CostType};
use entities::ids::{CostEntryId, FileAssetId, SalesOrderId, SalesOrderLineId, SupplierAccountId};
use entities::money::Amount;
use entities::money::Rate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 成本事实列表允许的排序字段白名单。
pub(crate) const COST_ENTRY_SORT_FIELDS: &[&str] =
    &["occurred_at", "gross_amount", "net_amount", "created_at"];
/// 成本分配列表允许的排序字段白名单。
pub(crate) const COST_ALLOCATION_SORT_FIELDS: &[&str] = &["created_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

// ---------------------------------------------------------------------------
// 成本事实（cost_entry）
// ---------------------------------------------------------------------------

/// 成本分配请求行（W16 手工成本入账；一期成本归属销售单必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CostAllocationLineRequest {
    /// 经营归属销售单。
    pub sales_order_id: SalesOrderId,
    /// 经营归属销售明细（可空）。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    #[serde(default)]
    pub rounding_residual_flag: Option<bool>,
}

/// 成本事实创建请求（手工成本入账：事实 + 分配行原子可见）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCostEntryRequest {
    /// 成本类型。
    pub cost_type: CostType,
    /// 成本阶段。
    pub cost_stage: CostStage,
    /// 成本归属范围。
    pub cost_scope: CostScope,
    /// 成本取值基础（二期商城消费必填；其他成本可空；`NONE` 不得入表）。
    pub cost_basis: Option<CostBasis>,
    /// 成本供应商（可空）。
    pub supplier_id: Option<SupplierAccountId>,
    /// 含税成本金额。
    pub gross_amount: Amount,
    /// 不含税成本金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 含税标识。
    pub tax_inclusion: bool,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 成本发生时间（秒级时间戳）。
    pub occurred_at: Instant,
    /// 来源事实类型。
    #[validate(custom(function = "non_blank", message = "来源事实类型不能为空"))]
    pub source_fact_type: String,
    /// 来源单据 ID。
    #[validate(custom(function = "non_blank", message = "来源单据ID不能为空"))]
    pub source_document_id: String,
    /// 来源行 ID。
    pub source_line_id: String,
    /// 来源版本。
    pub source_version: String,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 成本分配行（合计必须等于成本事实金额）。
    #[validate(length(min = 1, message = "至少提供一条成本分配"))]
    pub allocations: Vec<CostAllocationLineRequest>,
}

/// 成本分配响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CostAllocationView {
    /// 实体主键。
    pub id: String,
    /// 成本事实。
    pub cost_entry_id: String,
    /// 经营归属销售单。
    pub sales_order_id: Option<String>,
    /// 经营归属销售明细。
    pub sales_order_line_id: Option<String>,
    /// 二期消费成本归属。
    pub mall_consumption_entry_id: Option<String>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    pub rounding_residual_flag: bool,
}

/// 成本事实响应视图（W16 实际经营盈亏的成本明细行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CostEntryView {
    /// 实体主键。
    pub id: String,
    /// 成本类型。
    pub cost_type: CostType,
    /// 成本阶段。
    pub cost_stage: CostStage,
    /// 成本归属范围。
    pub cost_scope: CostScope,
    /// 成本取值基础。
    pub cost_basis: Option<CostBasis>,
    /// 成本供应商。
    pub supplier_id: Option<String>,
    /// 含税成本金额。
    pub gross_amount: Amount,
    /// 不含税成本金额（利润指标口径）。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 含税标识。
    pub tax_inclusion: bool,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 成本发生时间（秒级时间戳）。
    pub occurred_at: Instant,
    /// 来源事实类型。
    pub source_fact_type: String,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 来源行 ID。
    pub source_line_id: String,
    /// 来源版本。
    pub source_version: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 成本分配行。
    pub allocations: Vec<CostAllocationView>,
}

/// 成本事实列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CostEntryListParams {
    /// 成本类型筛选。
    pub cost_type: Option<CostType>,
    /// 成本阶段筛选。
    pub cost_stage: Option<CostStage>,
    /// 成本归属范围筛选。
    pub cost_scope: Option<CostScope>,
    /// 成本供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源单据 ID 模糊筛选。
    pub source_document_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`occurred_at`/`gross_amount`/`net_amount`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的成本事实列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CostEntryListQuery {
    /// 成本类型筛选。
    pub cost_type: Option<CostType>,
    /// 成本阶段筛选。
    pub cost_stage: Option<CostStage>,
    /// 成本归属范围筛选。
    pub cost_scope: Option<CostScope>,
    /// 成本供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源单据 ID 模糊筛选。
    pub source_document_id: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CostEntryListParams {
    /// 归一化成本事实列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CostEntryListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, COST_ENTRY_SORT_FIELDS)?;
        Ok(CostEntryListQuery {
            cost_type: self.cost_type,
            cost_stage: self.cost_stage,
            cost_scope: self.cost_scope,
            supplier_id: self.supplier_id.clone(),
            source_document_id: normalized_text(self.source_document_id.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 成本分配列表查询参数（按成本事实筛选）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CostAllocationListParams {
    /// 成本事实筛选。
    pub cost_entry_id: Option<CostEntryId>,
    /// 经营归属销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的成本分配列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CostAllocationListQuery {
    /// 成本事实筛选。
    pub cost_entry_id: Option<CostEntryId>,
    /// 经营归属销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CostAllocationListParams {
    /// 归一化成本分配列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CostAllocationListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, COST_ALLOCATION_SORT_FIELDS)?;
        Ok(CostAllocationListQuery {
            cost_entry_id: self.cost_entry_id.clone(),
            sales_order_id: self.sales_order_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, CostAllocationListParams, CostEntryListParams, SortDir};
    use entities::cost::CostStage;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn cost_entry_list_params_normalize_filters_and_paging() {
        let params = CostEntryListParams {
            cost_type: None,
            cost_stage: Some(CostStage::Actual),
            cost_scope: None,
            supplier_id: None,
            source_document_id: Some(" PO-1 ".to_string()),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("occurred_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.cost_stage, Some(CostStage::Actual));
        assert_eq!(query.source_document_id.as_deref(), Some("PO-1"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.sort_by, "occurred_at");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = CostEntryListParams {
            cost_type: None,
            cost_stage: None,
            cost_scope: None,
            supplier_id: None,
            source_document_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let allocations = CostAllocationListParams {
            cost_entry_id: None,
            sales_order_id: None,
            page: Some(1),
            page_size: Some(25),
            sort_by: Some("created_at".to_string()),
            sort_dir: None,
        };
        assert!(allocations.normalized().is_ok());
    }
}
