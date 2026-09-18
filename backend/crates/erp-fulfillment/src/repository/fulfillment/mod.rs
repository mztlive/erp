//! 域 D16 `fulfillment` 仓储：purchase_receipt(+_line)、delivery(+_line)、
//! electronic_delivery、service_fulfillment、customer_acceptance(+_line)、
//! acceptance_fulfillment_allocation（页面：W06、W09）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本目录按集合拆分投影行、筛选与
//! 域特有查询。跨集合批量 `$in` 查询见 `queries`，依赖事务的表头加行写入见
//! `writes`。集合名常量统一从 `FulfillmentExt` 关联常量导入。
//!
//! - [`purchase_receipt`]：采购入库单列表投影与按入库单号查询；
//! - [`purchase_receipt_totals`]：采购单已过账入库行的累计合格收货聚合
//!   （FUL-R01）；
//! - [`delivery`]：发货单列表投影与按物流单号查询；
//! - [`electronic_delivery`]：电子交付记录列表投影查询；
//! - [`service_fulfillment`]：线下服务履约记录列表投影查询；
//! - [`customer_acceptance`]：客户验收单列表投影与按验收单号查询；
//! - [`FulfillmentRepository`] 承载跨集合批量取行（`$in` 一次取回，禁止 N+1）
//!   与依赖事务的表头加行写入。
//!
//! 软删除边界（§4.5）：草稿单据（采购入库单等）可逻辑删除；已过账/已发货/
//! 已确认/已冲正及分配（`electronic_delivery`、`service_fulfillment`、
//! `acceptance_fulfillment_allocation`）是正式事实，**不提供软删除方法**
//! （基类通用方法不属于本域契约）。
//!
//! 五个集合的筛选类型定义在各子模块，经本模块根 re-export 后由 `FulfillmentExt`
//! 的关联类型对外暴露（`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs`
//! 增加 re-export）；投影行只作为公开搜索方法的返回类型使用，不在此处 re-export。

mod customer_acceptance;
mod delivery;
mod electronic_delivery;
mod filter;
mod purchase_receipt;
mod purchase_receipt_totals;
mod queries;
mod service_fulfillment;
mod writes;

use std::collections::HashMap;

pub use customer_acceptance::{CustomerAcceptanceFilter, CustomerAcceptanceRepositoryExt};
pub use delivery::{DeliveryFilter, DeliveryRepositoryExt};
pub use electronic_delivery::{ElectronicDeliveryFilter, ElectronicDeliveryRepositoryExt};
use erp_core::ids::{PurchaseOrderId, PurchaseOrderRevisionLineId};
use erp_core::money::Quantity;
pub(crate) use filter::{active_filter, page_and_size};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Result, mongo_ops};
pub use purchase_receipt::{PurchaseReceiptFilter, PurchaseReceiptRepositoryExt};
pub use service_fulfillment::{ServiceFulfillmentFilter, ServiceFulfillmentRepositoryExt};

use super::extensions::FulfillmentExt;

/// `purchase_receipt_line` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const PURCHASE_RECEIPT_LINES: &str = <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPT_LINES;
/// `delivery_line` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const DELIVERY_LINES: &str = <mongodb::Database as FulfillmentExt>::DELIVERY_LINES;
/// `customer_acceptance_line` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const CUSTOMER_ACCEPTANCE_LINES: &str = <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCE_LINES;
/// `acceptance_fulfillment_allocation` 集合名（单一来源：`FulfillmentExt` 关联常量）。
const ACCEPTANCE_FULFILLMENT_ALLOCATIONS: &str =
    <mongodb::Database as FulfillmentExt>::ACCEPTANCE_FULFILLMENT_ALLOCATIONS;

/// D16 域专用仓储：跨集合批量查询与多步骤事务写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型承载按表头批量取行（`$in`
/// 一次取回，禁止 N+1）与依赖事务的跨集合原子写入入口，由
/// `FulfillmentExt::fulfillment()` 访问。
pub struct FulfillmentRepository<'a> {
    db: &'a Database,
}

impl<'a> FulfillmentRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 统计采购单已过账入库的累计有效收货（按采购版本行分组）。
    ///
    /// 只在数据库内过滤未删除且 `POSTED` 的入库单，并按
    /// `purchase_order_revision_line_id` 聚合未删除入库行的
    /// `qualified_quantity`；不反序列化入库单/入库行整实体，也不随单据或
    /// 行数增长数据库访问次数。查询使用调用方执行器：事务内调用看到同一
    /// 事务的未提交写入；本方法不自行开启或提交事务。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回「采购版本行 → 累计合格数量」映射；无任何已过账未删除入库行时
    /// 返回空映射。
    ///
    /// # 错误
    /// 聚合或游标读取失败时返回错误；Decimal128 求和结果无法转换为
    /// `Quantity`（精度或上限越界）时返回错误而非 panic。
    pub async fn qualified_received_totals_by_purchase_revision_line(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<PurchaseOrderRevisionLineId, Quantity>> {
        purchase_receipt_totals::load_qualified_received_totals(self.db, purchase_order_id, executor).await
    }
}

/// 执行通用筛选分页投影查询（五类列表共用；查询语义与返回形状不变）。
///
/// # 参数
/// * `base` - 基集合句柄（用于计数）
/// * `filter` - 查询条件文档
/// * `sort` - 排序文档
/// * `skip` - 跳过行数
/// * `limit` - 单页条数
/// * `projection` - 投影文档
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回当前页投影行与满足筛选条件的总数.
///
/// # 错误
/// 当 MongoDB 查询、游标读取或计数失败时返回错误。
pub(super) async fn search_projected_page<Entity, Row>(
    base: &mongodb::Collection<Entity>,
    filter: mongodb::bson::Document,
    sort: mongodb::bson::Document,
    skip: u64,
    limit: i64,
    projection: mongodb::bson::Document,
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    Entity: Send + Sync,
    Row: for<'de> serde::Deserialize<'de> + serde::Serialize + Send + Sync,
{
    let options = FindOptions::builder().sort(sort).skip(skip).limit(limit).projection(projection).build();
    let collection = base.clone_with_type::<Row>();
    let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
    let total = mongo_ops::count_documents(base, filter, executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

/// 构建排序文档（字段名白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|field| allowed.contains(field)).unwrap_or("created_at");
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::sort_doc;

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_defaults_otherwise() {
        let allowed = ["created_at", "posted_at"];
        assert_eq!(sort_doc(None, false, &allowed), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("posted_at"), true, &allowed), doc! { "posted_at": 1 });
        assert_eq!(
            sort_doc(Some("任意字段"), false, &allowed),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序"
        );
    }

    #[test]
    fn filter_types_remain_reexported_at_module_root() {
        fn assert_reexported<T>() {}
        assert_reexported::<super::CustomerAcceptanceFilter>();
        assert_reexported::<super::DeliveryFilter>();
        assert_reexported::<super::ElectronicDeliveryFilter>();
        assert_reexported::<super::PurchaseReceiptFilter>();
        assert_reexported::<super::ServiceFulfillmentFilter>();
    }
}
