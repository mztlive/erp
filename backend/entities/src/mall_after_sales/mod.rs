//! 域 D30 `mall_after_sales`：mall_after_sales_request(+_line)、mall_refund(+_line)、
//! mall_refund_allocation、mall_balance_restoration(+_allocation)（页面：W25）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype、`entities::money` 定点类型
//! 与 `common` 基元。字段字典见数据模型 §6.18；公共字段归属按 §4.3 判定：
//! - `mall_after_sales_request` 是商城售后申请的稳定头，定义状态迁移
//!   （已接收 → 供应商处理中/退款处理中/待人工 → … → 已关闭，关闭条件从适用事实
//!   派生），实现 [`crate::common::state::DocumentState`]；
//! - `mall_refund`/`mall_refund_line`/`mall_refund_allocation`、
//!   `mall_balance_restoration`/`mall_balance_restoration_allocation` 是正式事实，
//!   §4.5 不设业务软删除，只 `new()`；
//! - 跨行守恒（退款行合计等于头金额、净 APPLY−REVERSE 分配合计等于行退款金额、
//!   每卡累计恢复不超过对应 CARD 退款净额）依赖聚合查询，由 P3 落实
//!   （P3 条目：§6.18 退款/恢复守恒与上限）。

pub mod after_sales_request;
pub mod balance_restoration;
pub mod consumption_refund_limit;
pub mod refund;
pub mod restoration_limit;
pub mod types;

pub use crate::ids::{
    MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallBalanceRestorationAllocationId,
    MallBalanceRestorationId, MallRefundAllocationId, MallRefundId, MallRefundLineId,
};
pub use after_sales_request::*;
pub use balance_restoration::*;
pub use consumption_refund_limit::*;
pub use refund::*;
pub use restoration_limit::*;
pub use types::*;
