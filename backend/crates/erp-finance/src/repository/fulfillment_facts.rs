//! 财务拥有的履约付款来源查询。

use crate::entity::payable::{PayableAccount, PayableSourceType};
use crate::repository::owned::PayableAccountRepository;
use erp_core::ids::PurchaseOrderId;
use persistence_core::{Executor, Result};

impl PayableAccountRepository<'_> {
    /// 查询采购单来源的应付往来子账。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回来源类型为采购单且来源主键匹配的应付子账。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_payable_accounts_for_purchase_order(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableAccount>> {
        self.find_many(
            mongodb::bson::doc! {
                "source_document_id": purchase_order_id.to_string(),
                "source_type": PayableSourceType::PurchaseOrder.as_str(),
                "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}
