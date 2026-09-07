//! 销售变更向组合层交付的冻结修订事实；不引用下游领域类型。

use crate::entity::sales_order::BusinessType;
use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use erp_core::money::Amount;

/// 已准备正式版本的稳定来源和金额，供下游消费方显式映射。
#[derive(Debug, Clone)]
pub struct SalesChangeRevisionFact {
    /// 原销售单稳定标识。
    pub sales_order_id: SalesOrderId,
    /// 新正式版本标识。
    pub revision_id: SalesOrderRevisionId,
    /// 销售自身业务分类。
    pub business_type: BusinessType,
    /// 当前正式版本含税金额。
    pub current_gross: Amount,
    /// 新正式公共行含税金额合计。
    pub new_gross: Amount,
    /// 构造本次正式版本时冻结的时间。
    pub posted_at: Instant,
}
