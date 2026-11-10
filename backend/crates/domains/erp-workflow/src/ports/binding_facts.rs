//! 绑定升级需要的当前订单事实；普通对象读权由各来源领域证明。
use super::OrderTaskSource;

/// 已确认存在的审批主体及其订单绑定事实；非订单类型不填写订单维度。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApprovalBindingFacts {
    pub order_source: Option<OrderTaskSource>,
    pub owner_user_id: String,
    pub business_org_unit_id: Option<String>,
}
