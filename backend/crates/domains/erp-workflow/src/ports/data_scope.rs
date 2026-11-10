//! 工作流消费公共解析后的对象判定，不接收原始规则或自行解释组织关系。

use std::collections::HashMap;

use super::OrderTaskSource;
use crate::entity::document_registry::DocumentType;

/// 业务适配器提供的当前对象事实；三个身份维度互不替代。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowScopeObject {
    /// 销售客户协作读取所需的当前客户事实。
    pub customer_id: Option<String>,
    /// 订单详情范围的独立来源；绑定前的本地事实可以省略。
    pub order_source: Option<OrderTaskSource>,
    pub owner_user_id: String,
    pub business_org_unit_id: Option<String>,
    pub settlement_party_id: Option<String>,
    pub warehouse_id: Option<String>,
}

/// 单批强业务对象范围事实。
pub type WorkflowScopeObjects = HashMap<(DocumentType, String), WorkflowScopeObject>;
