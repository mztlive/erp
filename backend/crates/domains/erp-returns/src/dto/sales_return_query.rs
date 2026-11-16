//! 销售退货单域查询 DTO。
use application_core::{SortDir, normalize_sort, normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use crate::entity::returns::{CaseType, ReturnRoute, SalesReturnCaseStatus};
const SALES_RETURN_CASE_SORT_FIELDS: &[&str] = &["discovered_at", "created_at"];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SalesReturnPageParams {
    pub page: u64,
    pub page_size: u32,
    pub sort_by: &'static str,
    pub sort_dir: SortDir,
}
/// 销售退货明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesReturnLineView {
    /// 实体主键。
    pub id: String,
    /// 原销售明细。
    pub sales_order_line_id: String,
    /// 申请退回数量。
    pub requested_quantity: Quantity,
    /// 实际退回数量。
    pub received_quantity: Option<Quantity>,
    /// 退回验收结果。
    pub quality_result: Option<String>,
    /// 可重新入库数量。
    pub restockable_quantity: Option<Quantity>,
}

/// 销售退货/拒收处理单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesReturnCaseView {
    /// 实体主键。
    pub id: String,
    /// 退货/拒收处理号。
    pub return_no: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 验收依据。
    pub acceptance_id: Option<String>,
    /// 处理类型。
    pub case_type: CaseType,
    /// 原因。
    pub reason: String,
    /// 发现时间（秒级时间戳）。
    pub discovered_at: Instant,
    /// 退货路线。
    pub return_route: ReturnRoute,
    /// 处理单状态。
    pub status: SalesReturnCaseStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 退货明细行。
    pub lines: Vec<SalesReturnLineView>,
}

/// 销售退货/拒收处理单列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct SalesReturnCaseListParams {
    /// 退货处理号模糊筛选。
    pub return_no: Option<String>,
    /// 原销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 处理单状态筛选。
    pub status: Option<SalesReturnCaseStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`discovered_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的销售退货处理单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesReturnCaseListQuery {
    /// 退货处理号模糊筛选。
    pub return_no: Option<String>,
    /// 原销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 处理单状态筛选。
    pub status: Option<SalesReturnCaseStatus>,
    /// 分页与排序参数。
    pub paging: SalesReturnPageParams,
}

impl SalesReturnCaseListParams {
    /// 归一化销售退货处理单列表查询参数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesReturnCaseListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SALES_RETURN_CASE_SORT_FIELDS)?;
        Ok(SalesReturnCaseListQuery {
            return_no: normalized_text(self.return_no.as_deref()),
            sales_order_id: self.sales_order_id.clone(),
            status: self.status,
            paging: SalesReturnPageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sales_return_case_list_params_normalize_filters() {
        let params = SalesReturnCaseListParams {
            return_no: Some(" RT-1 ".to_string()),
            status: Some(SalesReturnCaseStatus::Processing),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("discovered_at".to_string()),
            sort_dir: Some("asc".to_string()),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.return_no.as_deref(), Some("RT-1"));
        assert_eq!(query.status, Some(SalesReturnCaseStatus::Processing));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.sort_by, "discovered_at");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }
}
