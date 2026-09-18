//! 跨集合写入：表头加行与草稿验收行替换。
//!
//! 三个 `create_*_with_lines` 转调 `create_header_with_lines`，静态 tracing span
//! 名保持各自入口不变。

use erp_core::ids::CustomerAcceptanceId;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use super::{CUSTOMER_ACCEPTANCE_LINES, DELIVERY_LINES, FulfillmentRepository, PURCHASE_RECEIPT_LINES};
use crate::entity::fulfillment::{
    CustomerAcceptance, CustomerAcceptanceLine, Delivery, DeliveryLine, PurchaseReceipt, PurchaseReceiptLine,
};
use crate::repository::extensions::FulfillmentExt;

impl FulfillmentRepository<'_> {
    /// 创建采购入库单及全部行（跨集合多步骤写入）。
    ///
    /// # 参数
    /// * `receipt` - 待写入的入库单表头
    /// * `lines` - 待写入的入库行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.create_purchase_receipt_with_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "create_purchase_receipt_with_lines"
        )
    )]
    pub async fn create_purchase_receipt_with_lines(
        &self,
        receipt: &PurchaseReceipt,
        lines: &[PurchaseReceiptLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.create_header_with_lines(
            &self.db.collection::<PurchaseReceipt>(<mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS),
            receipt,
            &self.db.collection::<PurchaseReceiptLine>(PURCHASE_RECEIPT_LINES),
            lines,
            executor,
        )
        .await
    }

    /// 创建发货单及全部行（跨集合多步骤写入）。
    ///
    /// # 参数
    /// * `delivery` - 待写入的发货单表头
    /// * `lines` - 待写入的发货行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.create_delivery_with_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "create_delivery_with_lines"
        )
    )]
    pub async fn create_delivery_with_lines(
        &self,
        delivery: &Delivery,
        lines: &[DeliveryLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.create_header_with_lines(
            &self.db.collection::<Delivery>(<mongodb::Database as FulfillmentExt>::DELIVERIES),
            delivery,
            &self.db.collection::<DeliveryLine>(DELIVERY_LINES),
            lines,
            executor,
        )
        .await
    }

    /// 创建客户验收单及全部行（跨集合多步骤写入）。
    ///
    /// # 参数
    /// * `acceptance` - 待写入的验收单表头
    /// * `lines` - 待写入的验收行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.create_customer_acceptance_with_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.operation.name = "create_customer_acceptance_with_lines"
        )
    )]
    pub async fn create_customer_acceptance_with_lines(
        &self,
        acceptance: &CustomerAcceptance,
        lines: &[CustomerAcceptanceLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.create_header_with_lines(
            &self.db.collection::<CustomerAcceptance>(
                <mongodb::Database as FulfillmentExt>::CUSTOMER_ACCEPTANCES,
            ),
            acceptance,
            &self.db.collection::<CustomerAcceptanceLine>(CUSTOMER_ACCEPTANCE_LINES),
            lines,
            executor,
        )
        .await
    }

    /// 依次写入表头与行，保证表头与行原子可见（§6.7）。
    ///
    /// **必须收到事务执行器**：本方法不构成原子边界，传入
    /// `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有表头没有行
    /// 的半成品；Service 必须通过 `persistence_core::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `header_collection` - 表头集合
    /// * `header` - 待写入的表头
    /// * `lines_collection` - 行集合
    /// * `lines` - 待写入的行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 两笔写入均成功后返回 `Ok(())`。
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    async fn create_header_with_lines<Header, Line>(
        &self,
        header_collection: &mongodb::Collection<Header>,
        header: &Header,
        lines_collection: &mongodb::Collection<Line>,
        lines: &[Line],
        executor: &mut dyn Executor,
    ) -> Result<()>
    where
        Header: serde::Serialize + Sync + Send,
        Line: serde::Serialize + Clone + Sync + Send,
    {
        mongo_ops::insert_one(header_collection, header, executor).await?;
        mongo_ops::insert_many(lines_collection, lines.to_vec(), executor).await
    }

    /// 原子替换草稿客户验收单的全部行。
    ///
    /// 本方法只执行仓储写入，不判断表头状态；Service 必须先锁定并校验表头仍
    /// 为草稿，再传入同一个事务执行器。删除旧行与插入新行必须共同回滚。
    ///
    /// # 参数
    /// * `acceptance_id` - 草稿验收单主键
    /// * `lines` - 完整的新验收行集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当删除或批量插入失败时返回错误。
    #[tracing::instrument(
        name = "repository.fulfillment.replace_customer_acceptance_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "fulfillment",
            db.system.name = "mongodb",
            db.collection.name = "customer_acceptance_lines",
            db.operation.name = "replace"
        )
    )]
    pub async fn replace_customer_acceptance_lines(
        &self,
        acceptance_id: &CustomerAcceptanceId,
        lines: &[CustomerAcceptanceLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::delete_many(
            &self.db.collection::<CustomerAcceptanceLine>(CUSTOMER_ACCEPTANCE_LINES),
            doc! { "customer_acceptance_id": acceptance_id.to_string() },
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<CustomerAcceptanceLine>(CUSTOMER_ACCEPTANCE_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }
}
