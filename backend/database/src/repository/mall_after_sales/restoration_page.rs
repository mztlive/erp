//! 余额恢复列表数据库端分页与排序（INT-R09）。
//!
//! Service 先完成分页参数与排序白名单归一化；本文件只把已判定的售后案件
//! 作用域翻译为持久化过滤，并用数据库 `count + find` 返回当前页与总数，
//! 不做内存全量加载、排序或切片。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use entities::ids::MallAfterSalesRequestId;
use entities::mall_after_sales::MallBalanceRestoration;

use super::super::{PageResult, Pagination, QueryFilter};
use super::{MallBalanceRestorationRepository, MALL_BALANCE_RESTORATIONS};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 余额恢复列表允许的排序字段白名单（与 Service 校验保持一致）。
pub(crate) const RESTORATION_PAGE_SORT_FIELDS: &[&str] = &["restored_at", "created_at"];

/// 余额恢复分页查询条件（案件作用域 + 排序 + 分页已由 Service 判定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MallRestorationPageFilter {
    /// 同一售后案件。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 排序字段（白名单：`restored_at`/`created_at`，非法回落 `created_at`）。
    pub sort_by: String,
    /// 是否升序；`false` 表示降序。
    pub sort_ascending: bool,
    /// 页码（1 起；0 按第 1 页归一化）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl QueryFilter for MallRestorationPageFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回案件等值条件与未删除标记组成的查询文档。
    fn to_doc(&self) -> Document {
        doc! {
            "after_sales_request_id": self.after_sales_request_id.to_string(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        }
    }
}

impl Pagination for MallRestorationPageFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> MallBalanceRestorationRepository<'a> {
    /// 按售后案件返回恢复当前页与总数（INT-R09）。
    ///
    /// 数据库端执行 `count + find`：同一过滤条件计数总数，排序白名单映射后
    /// 取 `skip/limit` 页；排序含稳定 `id` 次键，同秒事实分页顺序确定。
    /// 全部使用调用方执行器；本方法不开启或提交事务。
    ///
    /// # 参数
    /// * `filter` - 已判定的案件作用域、排序与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页恢复事实与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    ///
    /// # 约束
    /// 软删除过滤、案件等值语义与稳定排序与旧内存实现一致；
    /// 不返回 services DTO、HTTP View 或授权结论。
    pub async fn search_page(
        &self,
        filter: &MallRestorationPageFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallBalanceRestoration>> {
        let filter_doc = filter.to_doc();
        let options = FindOptions::builder()
            .sort(restoration_sort_doc(&filter.sort_by, filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let collection = self
            .db
            .collection::<MallBalanceRestoration>(MALL_BALANCE_RESTORATIONS);
        let items = mongo_ops::find_many(&collection, filter_doc.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&collection, filter_doc, executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 构建恢复排序文档（白名单映射 + 稳定 `id` 次键同向）。
///
/// # 参数
/// * `sort_by` - 排序字段；不在白名单时回落到 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回含主字段与 `id` 次键的排序文档。
fn restoration_sort_doc(sort_by: &str, sort_ascending: bool) -> Document {
    let field = if RESTORATION_PAGE_SORT_FIELDS.contains(&sort_by) {
        sort_by
    } else {
        "created_at"
    };
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction, "id": direction }
}

#[cfg(test)]
mod tests {
    use super::{restoration_sort_doc, MallRestorationPageFilter, Pagination, QueryFilter};
    use entities::ids::MallAfterSalesRequestId;
    use mongodb::bson::doc;

    /// 案件过滤保留等值语义与未删除标记，分页偏移线性增长。
    #[test]
    fn restoration_page_filter_maps_case_and_pagination() {
        let filter = MallRestorationPageFilter {
            after_sales_request_id: MallAfterSalesRequestId::new("req-1"),
            sort_by: "restored_at".to_string(),
            sort_ascending: false,
            page: 2,
            page_size: 10,
        };
        let doc = filter.to_doc();
        assert_eq!(doc.get_str("after_sales_request_id").unwrap(), "req-1");
        assert_eq!(doc.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(filter.skip(), 10);
        assert_eq!(filter.limit(), 10);
    }

    /// 白名单外排序回落默认字段，稳定 `id` 次键与主方向一致。
    #[test]
    fn restoration_sort_doc_whitelists_and_keeps_stable_id_tiebreaker() {
        assert_eq!(
            restoration_sort_doc("restored_at", true),
            doc! { "restored_at": 1, "id": 1 }
        );
        assert_eq!(
            restoration_sort_doc("restored_at", false),
            doc! { "restored_at": -1, "id": -1 }
        );
        assert_eq!(
            restoration_sort_doc("created_at", true),
            doc! { "created_at": 1, "id": 1 }
        );
        assert_eq!(
            restoration_sort_doc("created_at", false),
            doc! { "created_at": -1, "id": -1 }
        );
        assert_eq!(
            restoration_sort_doc("malicious", true),
            doc! { "created_at": 1, "id": 1 },
            "白名单外字段必须回落到默认排序"
        );
    }

    /// 空页与尾页偏移可表示，零页归一化到第一页。
    #[test]
    fn restoration_page_pagination_handles_boundaries() {
        let filter = MallRestorationPageFilter {
            after_sales_request_id: MallAfterSalesRequestId::new("req-1"),
            sort_by: "created_at".to_string(),
            sort_ascending: false,
            page: 1,
            page_size: 20,
        };
        assert_eq!(filter.skip(), 0);
        let zero = MallRestorationPageFilter {
            page: 0,
            ..filter.clone()
        };
        assert_eq!(zero.skip(), 0);
        let beyond = MallRestorationPageFilter {
            page: 6,
            ..filter.clone()
        };
        assert_eq!(beyond.skip(), 100);
    }
}
