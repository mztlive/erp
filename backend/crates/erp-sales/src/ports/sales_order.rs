//! Facts required by sales order rules without depending on provider domains.

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SalesOrderId;
use erp_core::money::{Amount, Quantity, UnitPrice};
use persistence_core::Executor;

/// One receivable account's balances; zero accounts must remain distinguishable from an empty list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceivableBalanceFact {
    /// Unsettled amount of this individual account.
    pub open_total: Amount,
    /// Settled amount of this individual account.
    pub settled_total: Amount,
    /// Remaining invoiceable amount of this individual account.
    pub open_invoiceable_total: Amount,
    /// Invoiced amount of this individual account.
    pub invoiced_total: Amount,
}

/// Supply balances within the caller's transaction after sales existence has been checked.
#[async_trait]
pub trait SalesMoneyProgressPort: Send + Sync {
    /// Read every undeleted account for the sales order, without aggregating away zero accounts.
    ///
    /// Provider failures propagate before any sales progress write.
    async fn receivable_balances(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<ReceivableBalanceFact>>;
}

/// Exact SKU and revision identities accepted by the current catalog.
#[async_trait]
pub trait SellableSkuPort: Send + Sync {
    /// Return qualified pairs for the requested business date using the caller's executor.
    ///
    /// Missing pairs remain absent; repository failures must propagate unchanged in class.
    async fn qualified_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<(String, String)>>;
}

/// 自动报价需要的精确 SKU 修订与当前销售数量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesReferencePriceRequest {
    /// 稳定 SKU 身份。
    pub sku_id: String,
    /// 客户端锁定的精确 SKU 修订。
    pub sku_revision_id: String,
    /// 当前销售数量。
    pub quantity: Quantity,
}

/// 由公司 SKU 价格规则解析的含税参考单价。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesReferencePriceFact {
    /// 对应的精确价格请求。
    pub request: SalesReferencePriceRequest,
    /// 根据数量确定的参考单价。
    pub unit_price_gross: UnitPrice,
}

/// 销售自动报价的公司 SKU 参考价事实入口。
#[async_trait]
pub trait SalesReferencePricePort: Send + Sync {
    /// 按精确修订与数量返回可售 SKU 的参考价，缺失项保持缺失。
    ///
    /// # 参数
    /// * `requests` - 自动报价行的修订与数量
    /// * `date` - 销售资格业务日
    /// * `executor` - 调用方数据执行器
    ///
    /// # 返回
    /// 返回匹配成功的参考价事实；不得使用其他修订价格替代。
    ///
    /// # 错误
    /// 仓储与参考价转换失败原样传播。
    async fn reference_prices(
        &self,
        requests: &[SalesReferencePriceRequest],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<SalesReferencePriceFact>>;
}
