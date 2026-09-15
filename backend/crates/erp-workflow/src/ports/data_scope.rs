//! 工作流消费公共解析后的对象判定，不接收原始规则或自行解释组织关系。

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

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

/// 公共解析器产生的不可序列化对象判定接口。
pub trait WorkflowScopePredicate: Send + Sync {
    fn allows(&self, object: &WorkflowScopeObject) -> bool;
    /// 角色责任只能使用该角色自己的正向条款。
    fn allows_role(&self, _role: &str, _object: &WorkflowScopeObject) -> bool {
        false
    }
}

/// 已绑定资源、动作和版本的服务端范围；谓词只由生产 adapter 注入。
#[derive(Clone)]
pub struct WorkflowDataScope {
    pub resource: String,
    pub action: String,
    pub policy_version: u64,
    pub scope_version: String,
    pub granting_role_ids: Vec<String>,
    pub has_role_scope: bool,
    predicate: Arc<dyn WorkflowScopePredicate>,
}

impl WorkflowDataScope {
    /// 接收公共解析器的结果；不得由前端载荷构造。
    pub fn new(
        resource: String,
        action: String,
        policy_version: u64,
        scope_version: String,
        granting_role_ids: Vec<String>,
        has_role_scope: bool,
        predicate: Arc<dyn WorkflowScopePredicate>,
    ) -> Self {
        Self { resource, action, policy_version, scope_version, granting_role_ids, has_role_scope, predicate }
    }

    /// 按业务域提供的当前事实执行公共判定；不允许历史参与补充写权限。
    pub fn allows_role(&self, role: &str, object: &WorkflowScopeObject) -> bool {
        self.predicate.allows_role(role, object)
    }

    pub fn allows(&self, object: &WorkflowScopeObject) -> bool {
        self.predicate.allows(object)
    }
}

impl fmt::Debug for WorkflowDataScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkflowDataScope")
            .field("resource", &self.resource)
            .field("action", &self.action)
            .field("scope_version", &self.scope_version)
            .finish_non_exhaustive()
    }
}

impl PartialEq for WorkflowDataScope {
    fn eq(&self, other: &Self) -> bool {
        self.resource == other.resource
            && self.action == other.action
            && self.scope_version == other.scope_version
    }
}
impl Eq for WorkflowDataScope {}
