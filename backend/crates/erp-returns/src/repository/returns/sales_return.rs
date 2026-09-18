//! 销售退货处理单列表筛选/投影与处理单、明细集合扩展。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::stable::StableBase;
use erp_core::ids::{SalesOrderId, SalesReturnCaseId};
use mongodb::bson::{Document, doc};
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter,
};
use serde::{Deserialize, Serialize};

use super::search::{ListSort, search_projected};
use crate::entity::returns::{
    CaseType, ReturnRoute, SalesReturnCase, SalesReturnCaseStatus, SalesReturnLine,
};

/// 销售退货处理单列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesReturnCaseRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<SalesReturnCaseStatus>,
    /// 退货处理号。
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
    pub discovered_at: u64,
    /// 退货路线。
    pub return_route: ReturnRoute,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 销售退货处理单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesReturnCaseFilter {
    /// 退货处理号模糊匹配；`None` 表示不筛选。
    pub return_no: Option<String>,
    /// 原销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 处理单状态；`None` 表示不筛选。
    pub status: Option<SalesReturnCaseStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for SalesReturnCaseFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            return_no: None,
            sales_order_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for SalesReturnCaseFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "return_no", self.return_no.as_deref());
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SalesReturnCaseFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl ListSort for SalesReturnCaseFilter {
    fn sort_by(&self) -> Option<&str> {
        self.sort_by.as_deref()
    }

    fn sort_ascending(&self) -> bool {
        self.sort_ascending
    }
}

/// 销售退货处理单集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait SalesReturnCaseRepositoryExt {
    /// 分页检索销售退货处理单列表（投影查询）。
    ///
    /// 只返回 [`SalesReturnCaseRow`] 所需的列表字段；退货处理号支持字面量
    /// 模糊匹配（复用 `regex_filter`，禁止自拼正则）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_sales_return_cases(
        &self,
        filter: &SalesReturnCaseFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesReturnCaseRow>>;
}

impl SalesReturnCaseRepositoryExt for Repository<'_, SalesReturnCase> {
    async fn search_sales_return_cases(
        &self,
        filter: &SalesReturnCaseFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesReturnCaseRow>> {
        search_projected(
            self,
            filter,
            sales_return_case_projection(),
            &["discovered_at", "return_no", "created_at"],
            executor,
        )
        .await
    }
}

/// 销售退货明细集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait SalesReturnLineRepositoryExt {
    /// 批量按退货处理单集合取回明细（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `case_ids` - 退货处理单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_lines_by_cases(
        &self,
        case_ids: &[SalesReturnCaseId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesReturnLine>>;
}

impl SalesReturnLineRepositoryExt for Repository<'_, SalesReturnLine> {
    async fn find_lines_by_cases(
        &self,
        case_ids: &[SalesReturnCaseId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesReturnLine>> {
        if case_ids.is_empty() {
            return Ok(Vec::new());
        }
        let case_ids: Vec<String> = case_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "sales_return_case_id": { "$in": case_ids } }, executor).await
    }
}

/// 销售退货处理单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_return_case_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "return_no": 1,
        "sales_order_id": 1,
        "acceptance_id": 1,
        "case_type": 1,
        "reason": 1,
        "discovered_at": 1,
        "return_route": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::SalesReturnCaseFilter;
    use crate::entity::returns::CaseType;

    #[test]
    fn case_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesReturnCaseFilter {
            return_no: Some("RT-2026".to_string()),
            sales_order_id: Some(erp_core::ids::SalesOrderId::new("so-1")),
            status: Some(crate::entity::returns::SalesReturnCaseStatus::Processing),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(document.get_str("status").unwrap(), "processing");
        let regex = document.get_document("return_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), "RT\\-2026");
    }

    #[test]
    fn case_filter_type_field_roundtrips_through_entity_enum() {
        let _ = CaseType::Return;
        assert_eq!(CaseType::Shortage.as_str(), "shortage");
    }
}
