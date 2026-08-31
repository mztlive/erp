//! 域 D29 `mall_order`：mall_order_fact、mall_order_cancel_fact、mall_order_completion_fact、
//! mall_order、mall_order_item、mall_payment_source、mall_item_funding_allocation、
//! mall_consumption_entry、mall_consumption_cost_assessment（页面：W25、W28）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype、`entities::money` 定点类型
//! 与 `common` 基元。字段字典见数据模型 §6.17；公共字段归属按 §4.3 判定：
//! - 五类关键事实与消费、成本评估、分摊分配是正式事实（§4.5 不设业务软删除），
//!   但字典只给出 §6.17 明确的字段（`occurred_at`/`received_at`/`assessed_at` 等），
//!   不含 `fact_no`/`recorded_at`/`recorded_by`/`source_type` 等 FactBase 语义字段，
//!   因此按字典精确建模，不组合 [`crate::common::fact::FactBase`]；
//! - `mall_order` 是关键事实形成的追溯对象（§6.17），不是商城可变订单状态副本，
//!   只有 `attribution_status` 等归集进度字段可推进；
//! - 事实类实体只 `new()`；归集进度字段（`processing_status`/`attribution_status`）
//!   通过固定邻接的状态机推进（§13.3 禁止运行时扩展）。
//!
//! 跨域引用只使用 ID newtype（sku_id、customer_id、sales_order_id、卡实例相关 ID、
//! `cost_entry_id` 等），不 `use` 其他域实体类型。

pub mod consumption_entry;
pub mod cost_assessment;
pub mod funding_allocation;
pub mod funding_conservation;
pub mod order;
pub mod order_fact;
pub mod order_item;
pub mod payment_source;
pub mod types;

pub use crate::ids::{
    MallConsumptionCostAssessmentId, MallConsumptionEntryId, MallItemFundingAllocationId,
    MallOrderCancelFactId, MallOrderCompletionFactId, MallOrderFactId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId,
};
pub use consumption_entry::*;
pub use cost_assessment::*;
pub use funding_allocation::*;
pub use funding_conservation::*;
pub use order::*;
pub use order_fact::*;
pub use order_item::*;
pub use payment_source::*;
pub use types::*;
