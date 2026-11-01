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
    /// 判断当前对象是否落在已解析的正向范围内。
    ///
    /// # 参数
    /// * `object` - 业务适配器提供的当前对象事实。
    ///
    /// # 返回
    /// 允许访问时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    fn allows(&self, object: &WorkflowScopeObject) -> bool;
    /// 角色责任只能使用该角色自己的正向条款。
    ///
    /// # 参数
    /// * `role` - 待判定的角色 ID。
    /// * `object` - 当前对象事实。
    ///
    /// # 返回
    /// 该角色的正向条款允许时返回 `true`。默认实现返回 `false`。
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 参数
    /// * `resource` - 已解析的资源。
    /// * `action` - 已解析的动作。
    /// * `policy_version` - 策略版本。
    /// * `scope_version` - 范围版本。
    /// * `granting_role_ids` - 授予该范围的角色。
    /// * `has_role_scope` - 是否存在角色级范围。
    /// * `predicate` - 只由生产 adapter 注入的对象判定。
    ///
    /// # 返回
    /// 返回已绑定资源、动作和版本的范围。
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 参数
    /// * `role` - 待判定的角色 ID。
    /// * `object` - 当前对象事实。
    ///
    /// # 返回
    /// 委托谓词的 `allows_role`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn allows_role(&self, role: &str, object: &WorkflowScopeObject) -> bool {
        self.predicate.allows_role(role, object)
    }

    /// 按当前对象事实执行公共范围判定。
    ///
    /// # 参数
    /// * `object` - 当前对象事实。
    ///
    /// # 返回
    /// 委托谓词的 `allows`。
    ///
    /// # 错误
    /// 不返回错误。
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
