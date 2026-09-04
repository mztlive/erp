//! 域 D29 `mall_order` 服务编排（W25 商城消费订单、W28 卡券消费台账）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 消费入账（§8.4 第 3 条：支付事实 + 唯一订单 + 明细 + 支付来源 + 分摊矩阵
//!   加消费事实、成本评估与 `cost_entry`/`cost_allocation` 原子写入）→
//!   `database::Transactional::with_transaction`；
//! - 取消/完成事实（事实 + 一对一扩展表）→ 同一事务；
//! - 列表/详情只读查询 → `&mut NoTransaction`。
//!
//! 幂等（§6.17）：`business_fact_key`/`inbox_message_id` 唯一，重复接收返回
//! 既有正式事实，不产生第二份事实、订单、消费或成本（§9.4）。
//!
//! 跨域协作只调对方 Repository（P3-service-api.md §2）：
//! - D28 `CardInstanceExt`：切换 `T`（履约链归属）与卡实例（卡券归集）；
//! - D20 `CostExt`：`cost_entry`/`cost_allocation`（消费成本评估，§8.4 第 7 条）。

use mongodb::Database;

mod cost;
mod dto;
mod list_batch;
mod payment_plan;
mod query;
mod receive;
mod validated_fact_payload;

pub use self::dto::{
    ConservationResultRow, ConservationView, ConsumptionEntryView, CostAssessmentView,
    CostBasisBreakdownItemView, FactSummaryItemView, FundingAllocationView, MallOrderAddressView,
    MallOrderAmountsView, MallOrderCustomerView, MallOrderDetailView, MallOrderFactListParams,
    MallOrderFactView, MallOrderFulfillmentView, MallOrderIdentityView, MallOrderItemView,
    MallOrderListParams, MallOrderListRow, PageView, PaymentCompositionView, PaymentSourceView,
    ReceiveMallOrderFactRequest, ReceivedFactView, SupplierOrderSummaryView, SupplierOrderView,
};

/// 商城订单域服务：事实接收、消费入账、订单与事实查询。
pub struct MallOrderService {
    db: Database,
}

impl MallOrderService {
    /// 创建服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
