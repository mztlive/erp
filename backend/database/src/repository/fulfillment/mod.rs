//! 域 D16 `fulfillment` 仓储：purchase_receipt(+_line)、delivery(+_line)、
//! electronic_delivery、service_fulfillment、customer_acceptance(+_line)、
//! acceptance_fulfillment_allocation（页面：W06、W09）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本目录按集合拆分投影行、筛选与
//! 域特有查询，根模块承载跨集合批量查询与多步骤写入入口。集合名常量统一从
//! `FulfillmentExt` 关联常量导入。
//!
//! - [`purchase_receipt`]：采购入库单列表投影与按入库单号查询；
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
mod purchase_receipt;
mod service_fulfillment;

pub use customer_acceptance::CustomerAcceptanceFilter;
pub use delivery::DeliveryFilter;
pub use electronic_delivery::ElectronicDeliveryFilter;
pub use purchase_receipt::PurchaseReceiptFilter;
pub use service_fulfillment::ServiceFulfillmentFilter;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceLine, Delivery, DeliveryLine,
    FulfillmentFactType, PurchaseReceipt, PurchaseReceiptLine,
};
use entities::ids::{CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId, PurchaseReceiptId};

use super::extensions::FulfillmentExt;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

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

    /// 批量读取采购入库行（`$in` 一次取回，按行号升序）。
    ///
    /// 供单据详情/过账计算一次性加载全部行，禁止按表头逐条查询造成 N+1。
    ///
    /// # 参数
    /// * `receipt_ids` - 入库单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配行，按 `line_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn receipt_lines_by_receipt_ids(
        &self,
        receipt_ids: &[PurchaseReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReceiptLine>> {
        let mut lines = find_lines_in(
            self.db,
            PURCHASE_RECEIPT_LINES,
            "purchase_receipt_id",
            &ids_to_strings(receipt_ids),
            executor,
        )
        .await?;
        lines.sort_by_key(|line: &PurchaseReceiptLine| (line.purchase_receipt_id.to_string(), line.line_no));
        Ok(lines)
    }

    /// 批量读取发货行（`$in` 一次取回，按行号升序）。
    ///
    /// 供单据详情/发货过账计算一次性加载全部行，禁止按表头逐条查询造成 N+1。
    ///
    /// # 参数
    /// * `delivery_ids` - 发货单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配行，按 `line_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn delivery_lines_by_delivery_ids(
        &self,
        delivery_ids: &[DeliveryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<DeliveryLine>> {
        let mut lines = find_lines_in(
            self.db,
            DELIVERY_LINES,
            "delivery_id",
            &ids_to_strings(delivery_ids),
            executor,
        )
        .await?;
        lines.sort_by_key(|line: &DeliveryLine| (line.delivery_id.to_string(), line.line_no));
        Ok(lines)
    }

    /// 批量读取客户验收行（`$in` 一次取回，按行号升序）。
    ///
    /// 供单据详情/验收过账计算一次性加载全部行，禁止按表头逐条查询造成 N+1。
    ///
    /// # 参数
    /// * `acceptance_ids` - 验收单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配行，按 `line_no` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn acceptance_lines_by_acceptance_ids(
        &self,
        acceptance_ids: &[CustomerAcceptanceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAcceptanceLine>> {
        let mut lines = find_lines_in(
            self.db,
            CUSTOMER_ACCEPTANCE_LINES,
            "customer_acceptance_id",
            &ids_to_strings(acceptance_ids),
            executor,
        )
        .await?;
        lines.sort_by_key(|line: &CustomerAcceptanceLine| {
            (line.customer_acceptance_id.to_string(), line.line_no)
        });
        Ok(lines)
    }

    /// 批量读取验收履约分配（按验收行 `$in` 一次取回）。
    ///
    /// 供净验收数量（`APPLY - REVERSE`）计算一次性取回全部分配，禁止 N+1。
    ///
    /// # 参数
    /// * `acceptance_line_ids` - 验收行主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn allocations_by_acceptance_lines(
        &self,
        acceptance_line_ids: &[CustomerAcceptanceLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AcceptanceFulfillmentAllocation>> {
        find_lines_in(
            self.db,
            ACCEPTANCE_FULFILLMENT_ALLOCATIONS,
            "customer_acceptance_line_id",
            &ids_to_strings(acceptance_line_ids),
            executor,
        )
        .await
    }

    /// 批量读取验收履约分配（按履约事实 `$in` 一次取回）。
    ///
    /// 供关单「每履约事实净验收数量不超过净成功履约数量」校验取数，禁止 N+1。
    ///
    /// # 参数
    /// * `fact_type` - 履约事实类型（发货/电子交付/服务履约）
    /// * `fulfillment_line_ids` - 履约事实行主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn allocations_by_fulfillment_fact(
        &self,
        fact_type: FulfillmentFactType,
        fulfillment_line_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AcceptanceFulfillmentAllocation>> {
        if fulfillment_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let collection = self
            .db
            .collection::<AcceptanceFulfillmentAllocation>(ACCEPTANCE_FULFILLMENT_ALLOCATIONS);
        mongo_ops::find_many(
            &collection,
            doc! {
                "fulfillment_fact_type": fact_type.as_str(),
                "fulfillment_line_id": { "$in": fulfillment_line_ids },
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 创建采购入库单及全部行（跨集合多步骤写入）。
    ///
    /// 依次写入 `purchase_receipts` 与 `purchase_receipt_lines`，保证表头与行
    /// 原子可见（§6.7）。**必须收到事务执行器**：本方法不构成原子边界，传入
    /// `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有表头没有行的
    /// 半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `receipt` - 待写入的入库单表头
    /// * `lines` - 待写入的入库行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_purchase_receipt_with_lines(
        &self,
        receipt: &PurchaseReceipt,
        lines: &[PurchaseReceiptLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseReceipt>(<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS),
            receipt,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<PurchaseReceiptLine>(PURCHASE_RECEIPT_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }

    /// 创建发货单及全部行（跨集合多步骤写入）。
    ///
    /// 依次写入 `deliveries` 与 `delivery_lines`，保证表头与行原子可见（§6.7）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，中途失败会留下只有表头没有行的半成品；Service
    /// 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `delivery` - 待写入的发货单表头
    /// * `lines` - 待写入的发货行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_delivery_with_lines(
        &self,
        delivery: &Delivery,
        lines: &[DeliveryLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<Delivery>(<mongodb::Database as FulfillmentExt>::DELIVERIES),
            delivery,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<DeliveryLine>(DELIVERY_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }

    /// 创建客户验收单及全部行（跨集合多步骤写入）。
    ///
    /// 依次写入 `customer_acceptances` 与 `customer_acceptance_lines`，保证
    /// 表头与行原子可见（§6.7）。**必须收到事务执行器**：本方法不构成原子
    /// 边界，传入 `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有
    /// 表头没有行的半成品；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `acceptance` - 待写入的验收单表头
    /// * `lines` - 待写入的验收行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_customer_acceptance_with_lines(
        &self,
        acceptance: &CustomerAcceptance,
        lines: &[CustomerAcceptanceLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<CustomerAcceptance>(
                <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCES,
            ),
            acceptance,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<CustomerAcceptanceLine>(CUSTOMER_ACCEPTANCE_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }
}

/// 把 ID newtype 集合转为字符串集合（用于 `$in` 查询）。
///
/// # 参数
/// * `ids` - ID newtype 集合
///
/// # 返回
/// 返回字符串集合。
fn ids_to_strings<T: AsRef<str>>(ids: &[T]) -> Vec<String> {
    ids.iter().map(|id| id.as_ref().to_string()).collect()
}

/// 按给定字段 `$in` 批量读取行实体（空集合直接返回空列表）。
async fn find_lines_in<T>(
    db: &Database,
    collection_name: &str,
    field: &str,
    values: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let collection = db.collection::<T>(collection_name);
    mongo_ops::find_many(
        &collection,
        doc! {
            field: { "$in": values },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::default(),
        executor,
    )
    .await
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
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use super::{ids_to_strings, sort_doc};
    use mongodb::bson::doc;

    use entities::ids::PurchaseOrderId;

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_defaults_otherwise() {
        let allowed = ["created_at", "posted_at"];
        assert_eq!(sort_doc(None, false, &allowed), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("posted_at"), true, &allowed),
            doc! { "posted_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false, &allowed),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序"
        );
    }

    #[test]
    fn ids_to_strings_converts_newtype_collection() {
        let ids = vec![PurchaseOrderId::new("po-1"), PurchaseOrderId::new("po-2")];
        assert_eq!(ids_to_strings(&ids), vec!["po-1".to_string(), "po-2".to_string()]);
        assert!(ids_to_strings::<PurchaseOrderId>(&[]).is_empty());
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
