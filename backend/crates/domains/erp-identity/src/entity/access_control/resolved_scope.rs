//! 范围分支与限制条件共用的对象判定，不承载角色资格解析。

use std::collections::BTreeSet;

use super::ScopeDimension;
use crate::entity::authorization::AccessRule;

#[cfg(test)]
#[path = "resolved_scope_tests.rs"]
mod tests;

/// 单个资源动作的一组范围条件；内部归属条件取并集，再与其他维度取交集。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeClause {
    /// 公司级条款，无条件覆盖对象并满足必需维度。
    pub company: bool,
    /// 将本人负责纳入内部归属条件。
    pub self_owned: bool,
    /// 将本人协作参与纳入内部归属条件。
    pub collaborative: bool,
    /// 展开后的组织标识集合，与本人负责和协作条件任选其一。
    pub org_unit_ids: BTreeSet<String>,
    /// 可覆盖的结算主体标识；集合非空时须匹配对象的结算主体。
    pub settlement_party_ids: BTreeSet<String>,
    /// 可覆盖的仓库标识；集合非空时须匹配对象的仓库。
    pub warehouse_ids: BTreeSet<String>,
}

/// 服务端查询与对象判定共用的范围解析结果，不直接序列化给客户端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScope {
    /// 可任选其一的正向条款，包含解析后的人员范围或政策授予的公司范围。
    pub role_clauses: Vec<ScopeClause>,
    /// 正向条款和历史分支共用的个人上限；缺省时不额外收窄，空条款则全部拒绝。
    pub user_limit: Option<ScopeClause>,
}

/// 业务域针对本次账号和判断时点提供的对象事实；历史归属快照不得用作参与事实。
pub struct ScopedObject<'a> {
    /// 本次账号是否负责该对象。
    pub owned: bool,
    /// 本次账号是否以协作者身份参与该对象。
    pub collaborating: bool,
    /// 本次账号是否有可用于补充读取的真实历史参与事实。
    pub historical_read_participant: bool,
    /// 对象归属的内部组织标识；缺省时不能匹配组织集合。
    pub org_unit_id: Option<&'a str>,
    /// 对象所属的结算主体标识；缺省时不能匹配结算主体集合。
    pub settlement_party_id: Option<&'a str>,
    /// 对象所属的仓库标识；缺省时不能匹配仓库集合。
    pub warehouse_id: Option<&'a str>,
}

impl ScopeClause {
    /// 用业务对象事实检查单条范围：公司级直接覆盖，其余条款须命中所有已配置维度，空条款拒绝。
    pub fn covers(&self, object: &ScopedObject<'_>) -> bool {
        AccessRule::clause(self).allows_scope(object)
    }

    /// 检查单条范围是否提供动作的每个必需维度；公司级直接满足，其他条款须各自提供完整维度。
    pub(crate) fn complete(&self, dimensions: &[ScopeDimension]) -> bool {
        self.company
            || dimensions.iter().all(|dimension| match dimension {
                ScopeDimension::InternalOrg => {
                    self.self_owned || self.collaborative || !self.org_unit_ids.is_empty()
                },
                ScopeDimension::SettlementParty => !self.settlement_party_ids.is_empty(),
                ScopeDimension::Warehouse => !self.warehouse_ids.is_empty(),
            })
    }
}

impl ResolvedScope {
    /// 账号有效且完整动作权限成立后，对业务对象事实执行读取或写入范围判断。
    /// 任一正向条款命中即可进入上限检查；`allow_history` 启用时，历史参与也可独立进入该检查。
    /// 存在个人上限时，对象还须满足该上限。
    pub fn allows(&self, object: &ScopedObject<'_>, allow_history: bool) -> bool {
        AccessRule::scope(self, allow_history).allows_scope(object)
    }

    /// 检查是否至少有一条正向条款；个人上限只收窄结果，不能独立授予范围或补成公司级。
    pub fn has_role_scope(&self) -> bool {
        !self.role_clauses.is_empty()
    }
}
