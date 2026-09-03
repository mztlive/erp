//! 域 D21 `returns` 仓储：sales_return_case、sales_return_line、
//! purchase_return_order、purchase_return_line、customer_refund、supplier_refund、
//! receipt_reversal、payment_reversal。
//!
//! 单一集合 CRUD 直接复用 [`Repository`] 基类；本文件只补充域特有查询与
//! 跨集合多步骤事务写入入口。集合名常量统一从 `indexes::returns` 导入。
//!
//! 本域全部集合是退货/退款/冲正事实与处理单（§4.5），**不提供软删除方法**
//! （纠错用反向事实表达）。筛选/行类型定义在本文件，经 `ReturnsExt` 的关联
//! 类型对外暴露（`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加
//! re-export）。

use entities::common::stable::StableBase;
use entities::common::time::Instant;
use entities::ids::{
    CustomerAccountId, CustomerReceiptId, PurchaseOrderId, ReceivableEntryId, SalesOrderId, SupplierPaymentId,
};
use entities::returns::{
    CaseType, CustomerRefund, CustomerRefundStatus, PaymentReversal, PurchaseReturnLine, PurchaseReturnOrder,
    PurchaseReturnStatus, ReceiptReversal, ReturnMode, ReturnRoute, SalesReturnCase, SalesReturnCaseStatus,
    SalesReturnLine, SupplierRefund,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::ReturnsExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `sales_return_line` 集合名（单一来源：`ReturnsExt` 关联常量）。
const SALES_RETURN_LINES: &str = <mongodb::Database as ReturnsExt>::SALES_RETURN_LINES;
/// `purchase_return_line` 集合名（单一来源：`ReturnsExt` 关联常量）。
const PURCHASE_RETURN_LINES: &str = <mongodb::Database as ReturnsExt>::PURCHASE_RETURN_LINES;

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

/// 采购退货单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseReturnOrderRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<PurchaseReturnStatus>,
    /// 采购退货单号。
    pub purchase_return_no: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 客户侧依据。
    pub sales_return_case_id: Option<String>,
    /// 退货模式。
    pub return_mode: ReturnMode,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购退货单列表筛选条件。
#[derive(Debug, Clone)]
pub struct PurchaseReturnOrderFilter {
    /// 采购退货单号模糊匹配；`None` 表示不筛选。
    pub purchase_return_no: Option<String>,
    /// 原采购单；`None` 表示不筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 退货单状态；`None` 表示不筛选。
    pub status: Option<PurchaseReturnStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PurchaseReturnOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(
            &mut filter,
            "purchase_return_no",
            self.purchase_return_no.as_deref(),
        );
        if let Some(purchase_order_id) = &self.purchase_order_id {
            filter.insert("purchase_order_id", purchase_order_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PurchaseReturnOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 客户退款列表投影行。
///
/// 覆盖客户退款 View 所需退款事实，但不包含审批 View 或 Service DTO。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerRefundRow {
    /// 实体主键。
    pub id: String,
    /// 退款状态。
    pub status: CustomerRefundStatus,
    /// 退款单号。
    pub refund_no: String,
    /// 销售退货/拒收处理单。
    pub sales_return_case_id: Option<String>,
    /// 客户。
    pub customer_id: String,
    /// 原回款。
    pub original_receipt_id: Option<String>,
    /// 原应收分录。
    pub original_receivable_entry_id: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明。
    pub reason_text: String,
    /// 退款金额。
    pub amount: entities::money::Amount,
    /// 财务经办人。
    pub handled_by: String,
    /// 财务复核人。
    pub reviewed_by: String,
    /// 实际退款时间。
    pub occurred_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户退款列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerRefundFilter {
    /// 退款单号模糊匹配；`None` 表示不筛选。
    pub refund_no: Option<String>,
    /// 客户；`None` 表示不筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 退款状态；`None` 表示不筛选。
    pub status: Option<CustomerRefundStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CustomerRefundFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "refund_no", self.refund_no.as_deref());
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CustomerRefundFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesReturnCase> {
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
    pub async fn search_sales_return_cases(
        &self,
        filter: &SalesReturnCaseFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesReturnCaseRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["discovered_at", "return_no", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_return_case_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesReturnCaseRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, SalesReturnLine> {
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
    pub async fn find_lines_by_cases(
        &self,
        case_ids: &[entities::ids::SalesReturnCaseId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesReturnLine>> {
        if case_ids.is_empty() {
            return Ok(Vec::new());
        }
        let case_ids: Vec<String> = case_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "sales_return_case_id": { "$in": case_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, PurchaseReturnOrder> {
    /// 分页检索采购退货单列表（投影查询）。
    ///
    /// 只返回 [`PurchaseReturnOrderRow`] 所需的列表字段；采购退货单号支持
    /// 字面量模糊匹配。
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
    pub async fn search_purchase_return_orders(
        &self,
        filter: &PurchaseReturnOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PurchaseReturnOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["purchase_return_no", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(purchase_return_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<PurchaseReturnOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, PurchaseReturnLine> {
    /// 批量按退货单集合取回明细（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `order_ids` - 采购退货单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_lines_by_orders(
        &self,
        order_ids: &[entities::ids::PurchaseReturnOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReturnLine>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let order_ids: Vec<String> = order_ids.iter().map(ToString::to_string).collect();
        self.find_many(
            doc! { "purchase_return_order_id": { "$in": order_ids } },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, CustomerRefund> {
    /// 分页检索客户退款列表（投影查询）。
    ///
    /// 只返回 [`CustomerRefundRow`] 所需的列表字段；退款单号支持字面量模糊匹配。
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
    pub async fn search_customer_refunds(
        &self,
        filter: &CustomerRefundFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerRefundRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["occurred_at", "amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_refund_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerRefundRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量按原事实取回客户退款（`$in`，用于累计冲正校验）。
    ///
    /// # 参数
    /// * `receipt_ids` - 原回款 ID 集合（可为空）
    /// * `entry_ids` - 原应收分录 ID 集合（可为空）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配退款。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_refunds_by_originals(
        &self,
        receipt_ids: &[CustomerReceiptId],
        entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerRefund>> {
        let mut filter = Document::new();
        if !receipt_ids.is_empty() {
            let ids: Vec<String> = receipt_ids.iter().map(ToString::to_string).collect();
            filter.insert("original_receipt_id", doc! { "$in": ids });
        }
        if !entry_ids.is_empty() {
            let ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
            filter.insert("original_receivable_entry_id", doc! { "$in": ids });
        }
        self.find_many(filter, executor).await
    }
}

impl<'a> Repository<'a, SupplierRefund> {
    /// 批量按原事实取回供应商退款（`$in`，用于累计冲正校验）。
    ///
    /// # 参数
    /// * `payment_ids` - 原付款 ID 集合（可为空）
    /// * `entry_ids` - 原应付分录 ID 集合（可为空）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配退款。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_refunds_by_originals(
        &self,
        payment_ids: &[SupplierPaymentId],
        entry_ids: &[entities::ids::PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefund>> {
        let mut filter = Document::new();
        if !payment_ids.is_empty() {
            let ids: Vec<String> = payment_ids.iter().map(ToString::to_string).collect();
            filter.insert("original_payment_id", doc! { "$in": ids });
        }
        if !entry_ids.is_empty() {
            let ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
            filter.insert("original_payable_entry_id", doc! { "$in": ids });
        }
        self.find_many(filter, executor).await
    }
}

impl<'a> Repository<'a, ReceiptReversal> {
    /// 批量按原回款集合取回冲正单（`$in`，用于累计有效冲正校验）。
    ///
    /// # 参数
    /// * `receipt_ids` - 原客户回款 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配冲正单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_reversals_by_receipts(
        &self,
        receipt_ids: &[CustomerReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptReversal>> {
        if receipt_ids.is_empty() {
            return Ok(Vec::new());
        }
        let receipt_ids: Vec<String> = receipt_ids.iter().map(ToString::to_string).collect();
        self.find_many(
            doc! { "original_customer_receipt_id": { "$in": receipt_ids } },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, PaymentReversal> {
    /// 批量按原付款集合取回冲正单（`$in`，用于累计有效冲正校验）。
    ///
    /// # 参数
    /// * `payment_ids` - 原供应商付款 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配冲正单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_reversals_by_payments(
        &self,
        payment_ids: &[SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentReversal>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let payment_ids: Vec<String> = payment_ids.iter().map(ToString::to_string).collect();
        self.find_many(
            doc! { "original_supplier_payment_id": { "$in": payment_ids } },
            executor,
        )
        .await
    }
}

/// D21 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `ReturnsExt::returns()` 访问。
pub struct ReturnsRepository<'a> {
    db: &'a Database,
}

impl<'a> ReturnsRepository<'a> {
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

    /// 建立销售退货处理单与其明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `sales_return_cases` 与 `sales_return_lines`，保证「处理单 + 明细」
    /// 原子可见（数据模型 §6.11）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有处理单没有明细的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `case_entity` - 待写入的处理单
    /// * `line` - 待写入的明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sales_return_with_line(
        &self,
        case_entity: &SalesReturnCase,
        line: &SalesReturnLine,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesReturnCase>(<mongodb::Database as ReturnsExt>::SALES_RETURN_CASES),
            case_entity,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<SalesReturnLine>(SALES_RETURN_LINES),
            line,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 建立采购退货单与其明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `purchase_return_orders` 与 `purchase_return_lines`，保证
    /// 「退货单 + 明细」原子可见（数据模型 §6.11）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有退货单没有明细的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `order` - 待写入的采购退货单
    /// * `line` - 待写入的明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_purchase_return_with_line(
        &self,
        order: &PurchaseReturnOrder,
        line: &PurchaseReturnLine,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseReturnOrder>(<mongodb::Database as ReturnsExt>::PURCHASE_RETURN_ORDERS),
            order,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<PurchaseReturnLine>(PURCHASE_RETURN_LINES),
            line,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档：字段名经白名单映射，未命中回退 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段（白名单内有效）
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段名集合（防止透传任意字段名）
///
/// # 返回
/// 返回排序条件文档；`id` 作为次键保证同值稳定排序。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|name| allowed.contains(name))
        .unwrap_or("created_at");
    doc! { field: direction, "id": direction }
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

/// 采购退货单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn purchase_return_order_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "purchase_return_no": 1,
        "purchase_order_id": 1,
        "sales_return_case_id": 1,
        "return_mode": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 客户退款列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_refund_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "refund_no": 1,
        "sales_return_case_id": 1,
        "customer_id": 1,
        "original_receipt_id": 1,
        "original_receivable_entry_id": 1,
        "reason_code": 1,
        "reason_text": 1,
        "amount": 1,
        "handled_by": 1,
        "reviewed_by": 1,
        "occurred_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        customer_refund_projection, sort_doc, CustomerRefundFilter, PurchaseReturnOrderFilter, QueryFilter,
        SalesReturnCaseFilter,
    };
    use entities::returns::{CaseType, CustomerRefundStatus, PurchaseReturnStatus};
    use mongodb::bson::doc;

    #[test]
    fn case_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesReturnCaseFilter {
            return_no: Some("RT-2026".to_string()),
            sales_order_id: Some(entities::ids::SalesOrderId::new("so-1")),
            status: Some(entities::returns::SalesReturnCaseStatus::Processing),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(document.get_str("status").unwrap(), "processing");
        let regex = document.get_document("return_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), "RT\\-2026");
    }

    #[test]
    fn purchase_return_filter_filters_by_order_and_status() {
        let filter = PurchaseReturnOrderFilter {
            purchase_return_no: None,
            purchase_order_id: Some(entities::ids::PurchaseOrderId::new("po-1")),
            status: Some(PurchaseReturnStatus::Returned),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("purchase_order_id").unwrap(), "po-1");
        assert_eq!(document.get_str("status").unwrap(), "returned");
    }

    #[test]
    fn refund_filter_escapes_regex_and_sort_whitelist_falls_back() {
        let filter = CustomerRefundFilter {
            refund_no: Some("RF-1.1".to_string()),
            customer_id: None,
            status: Some(CustomerRefundStatus::Posted),
            page: 1,
            page_size: 20,
            sort_by: Some("handled_by".to_string()),
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let regex = document.get_document("refund_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), r"RF\-1\.1");
        assert_eq!(
            sort_doc(filter.sort_by.as_deref(), false, &["occurred_at", "amount"]),
            doc! { "created_at": -1, "id": -1 }
        );
        assert_eq!(filter.to_doc().get_str("status").unwrap(), "posted");
    }

    #[test]
    fn sort_doc_appends_id_tiebreaker_for_both_directions() {
        assert_eq!(
            sort_doc(
                Some("occurred_at"),
                true,
                &["occurred_at", "amount", "created_at"]
            ),
            doc! { "occurred_at": 1, "id": 1 }
        );
        assert_eq!(
            sort_doc(Some("amount"), false, &["occurred_at", "amount", "created_at"]),
            doc! { "amount": -1, "id": -1 }
        );
    }

    #[test]
    fn customer_refund_projection_covers_view_facts_without_approval() {
        let projection = customer_refund_projection();
        for field in [
            "id",
            "status",
            "refund_no",
            "sales_return_case_id",
            "customer_id",
            "original_receipt_id",
            "original_receivable_entry_id",
            "reason_code",
            "reason_text",
            "amount",
            "handled_by",
            "reviewed_by",
            "occurred_at",
            "version",
            "created_at",
        ] {
            assert_eq!(projection.get_i32(field).unwrap(), 1, "{field}");
        }
        assert!(projection.get("approval").is_none());
        assert!(projection.get("evidence_attachment_id").is_none());
    }

    #[test]
    fn case_filter_type_field_roundtrips_through_entity_enum() {
        let _ = CaseType::Return;
        assert_eq!(CaseType::Shortage.as_str(), "shortage");
    }
}

#[cfg(test)]
#[path = "returns_customer_refund_search.rs"]
mod customer_refund_search_tests;
