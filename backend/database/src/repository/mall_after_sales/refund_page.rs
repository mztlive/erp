//! 退款列表数据库端分页与排序（INT-R08）。
//!
//! Service 先通过 `MallRefundListQuery::scope` 判定唯一业务作用域与排序白名单；
//! 本文件只把已决定的作用域翻译为持久化过滤，并用数据库 `count + find` 返回
//! 当前页与总数，不做内存全量加载、排序或切片。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use entities::ids::{MallAfterSalesRequestId, MallOrderId};
use entities::mall_after_sales::MallRefund;

use super::super::{PageResult, Pagination, QueryFilter};
use super::{MallRefundRepository, MALL_REFUNDS};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 退款列表允许的排序字段白名单（与 Service 校验保持一致）。
pub(crate) const REFUND_PAGE_SORT_FIELDS: &[&str] = &["refunded_at", "created_at"];

/// Service 已判定的退款分页作用域（INT-R08）。
///
/// Repository 不拥有作用域优先级规则，只接收 Service 已决定的单一领域 ID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MallRefundPageScope {
    /// 按原商城订单读取退款事实。
    Order(MallOrderId),
    /// 按售后案件读取退款事实。
    AfterSalesRequest(MallAfterSalesRequestId),
}

/// 退款分页查询条件（作用域 + 排序 + 分页已由 Service 判定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MallRefundPageFilter {
    /// 已决定的单一业务作用域。
    pub scope: MallRefundPageScope,
    /// 排序字段（白名单：`refunded_at`/`created_at`，非法回落 `created_at`）。
    pub sort_by: String,
    /// 是否升序；`false` 表示降序。
    pub sort_ascending: bool,
    /// 页码（1 起；0 按第 1 页归一化）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl QueryFilter for MallRefundPageFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回作用域等值条件与未删除标记组成的查询文档。
    fn to_doc(&self) -> Document {
        match &self.scope {
            MallRefundPageScope::Order(order_id) => doc! {
                "mall_order_id": order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            MallRefundPageScope::AfterSalesRequest(request_id) => doc! {
                "after_sales_request_id": request_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
        }
    }
}

impl Pagination for MallRefundPageFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> MallRefundRepository<'a> {
    /// 按已判定作用域返回退款当前页与总数（INT-R08）。
    ///
    /// 数据库端执行 `count + find`：同一过滤条件计数总数，排序白名单映射后
    /// 取 `skip/limit` 页；排序含稳定 `id` 次键，同秒事实分页顺序确定。
    /// 全部使用调用方执行器；本方法不开启或提交事务。
    ///
    /// # 参数
    /// * `filter` - 已判定的作用域、排序与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页退款事实与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    ///
    /// # 约束
    /// 软删除过滤、作用域等值语义与稳定排序与旧内存实现一致；
    /// 不返回 services DTO、HTTP View 或授权结论。
    /// 本批次不新增索引：`mall_order_id` 专用查询索引留待冻结文件地基修订，
    /// 作用域等值过滤本身有界（单订单/单案件）。
    pub async fn search_page(
        &self,
        filter: &MallRefundPageFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallRefund>> {
        let filter_doc = filter.to_doc();
        let options = FindOptions::builder()
            .sort(refund_sort_doc(&filter.sort_by, filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let collection = self.db.collection::<MallRefund>(MALL_REFUNDS);
        let items = mongo_ops::find_many(&collection, filter_doc.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&collection, filter_doc, executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 构建退款排序文档（白名单映射 + 稳定 `id` 次键同向）。
///
/// # 参数
/// * `sort_by` - 排序字段；不在白名单时回落到 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回含主字段与 `id` 次键的排序文档。
fn refund_sort_doc(sort_by: &str, sort_ascending: bool) -> Document {
    let field = if REFUND_PAGE_SORT_FIELDS.contains(&sort_by) {
        sort_by
    } else {
        "created_at"
    };
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction, "id": direction }
}

#[cfg(test)]
mod tests {
    use super::{refund_sort_doc, MallRefundPageFilter, MallRefundPageScope, Pagination, QueryFilter};
    use entities::ids::{MallAfterSalesRequestId, MallOrderId};
    use mongodb::bson::doc;

    /// 订单与案件作用域分别生成等值过滤并保留未删除标记。
    #[test]
    fn refund_page_filter_maps_scope_and_deleted_filter() {
        let order = MallRefundPageFilter {
            scope: MallRefundPageScope::Order(MallOrderId::new("order-1")),
            sort_by: "refunded_at".to_string(),
            sort_ascending: false,
            page: 1,
            page_size: 20,
        };
        let doc = order.to_doc();
        assert_eq!(doc.get_str("mall_order_id").unwrap(), "order-1");
        assert_eq!(doc.get_i64("deleted_at").unwrap(), 0);

        let request = MallRefundPageFilter {
            scope: MallRefundPageScope::AfterSalesRequest(MallAfterSalesRequestId::new("req-1")),
            sort_by: "created_at".to_string(),
            sort_ascending: true,
            page: 2,
            page_size: 10,
        };
        let doc = request.to_doc();
        assert_eq!(doc.get_str("after_sales_request_id").unwrap(), "req-1");
        assert_eq!(request.skip(), 10);
        assert_eq!(request.limit(), 10);
    }

    /// 白名单外排序回落默认字段，稳定 `id` 次键与主方向一致。
    #[test]
    fn refund_sort_doc_whitelists_and_keeps_stable_id_tiebreaker() {
        assert_eq!(
            refund_sort_doc("refunded_at", false),
            doc! { "refunded_at": -1, "id": -1 }
        );
        assert_eq!(
            refund_sort_doc("created_at", true),
            doc! { "created_at": 1, "id": 1 }
        );
        assert_eq!(
            refund_sort_doc("malicious", false),
            doc! { "created_at": -1, "id": -1 },
            "白名单外字段必须回落到默认排序"
        );
    }

    /// 越界页偏移按页码线性增长，零页归一化到第一页。
    #[test]
    fn refund_page_pagination_offsets_grow_linearly() {
        let filter = MallRefundPageFilter {
            scope: MallRefundPageScope::Order(MallOrderId::new("order-1")),
            sort_by: "created_at".to_string(),
            sort_ascending: false,
            page: 3,
            page_size: 20,
        };
        assert_eq!(filter.skip(), 40);
        let first = MallRefundPageFilter {
            page: 0,
            ..filter.clone()
        };
        assert_eq!(first.skip(), 0);
    }

    /// 尾页与越界页偏移可表示；全部排序字段双向均映射稳定次键。
    ///
    /// 测试覆盖空页（第 1 页偏移 0）、尾页与越界页，以及 `refunded_at`/`created_at`
    /// 双向排序，保证分页边界与排序组合均有确定语义。
    #[test]
    fn refund_page_handles_tail_and_out_of_range_with_all_sorts() {
        use super::refund_sort_doc;
        let base = MallRefundPageFilter {
            scope: MallRefundPageScope::AfterSalesRequest(MallAfterSalesRequestId::new("req-1")),
            sort_by: "created_at".to_string(),
            sort_ascending: false,
            page: 1,
            page_size: 20,
        };
        assert_eq!(base.skip(), 0);
        let tail = MallRefundPageFilter {
            page: 5,
            ..base.clone()
        };
        assert_eq!(tail.skip(), 80);
        let beyond = MallRefundPageFilter {
            page: 101,
            ..base.clone()
        };
        assert_eq!(beyond.skip(), 2000);
        assert_eq!(
            refund_sort_doc("refunded_at", true),
            doc! { "refunded_at": 1, "id": 1 }
        );
        assert_eq!(
            refund_sort_doc("created_at", false),
            doc! { "created_at": -1, "id": -1 }
        );
    }
}
