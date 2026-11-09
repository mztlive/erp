use std::collections::BTreeSet;

use erp_core::validation::normalize_required_text_ref;

use super::{GrantSource, ResourceKind};
use crate::entity::access_control::{ResolvedScope, ScopedObject};
use crate::{Error, Permission, Result};

/// 来源管理页；next_cursor 为下一次读取的排他主键游标。
#[derive(Debug, Clone)]
pub struct GrantSourcePage {
    pub items: Vec<GrantSource>,
    pub next_cursor: Option<String>,
}

/// 已完成入口检查的列表查询约束；不装载账号全部授权 ID。
#[derive(Debug, Clone)]
pub struct AuthorizedResourceQuery {
    pub(crate) user_id: String,
    pub(crate) resource: ResourceKind,
    pub(crate) scope: ResolvedScope,
    pub(crate) scope_version: String,
    pub(crate) grant_revision: u64,
}
impl AuthorizedResourceQuery {
    /// 提供原数据范围及版本，指定记录授权由数据库关联条件补充。
    /// # 参数
    /// 无。
    /// # 返回
    /// 账号、资源、原范围及包含授权修订号的版本。
    /// # 错误
    /// 不返回错误；原范围不能单独作为完整列表授权条件。
    pub fn constraints(&self) -> (&str, ResourceKind, &ResolvedScope, &str) {
        (&self.user_id, self.resource, &self.scope, &self.scope_version)
    }
}

/// 本次需要检查的目标集合；不得用它表示账号的完整授权集合。
#[derive(Debug, Clone)]
pub struct ResourceTargets(BTreeSet<String>);
impl ResourceTargets {
    /// 校验目标身份并去重；数量只受本次操作规模约束。
    /// # 参数
    /// ids 为服务端确定的完整操作目标，最多 1000 项。
    /// # 返回
    /// 排序后的唯一目标，空集不查询授权集合。
    /// # 错误
    /// 超限、空白、超长或未规范化身份拒绝。
    pub fn new(ids: &[&str]) -> Result<Self> {
        if ids.len() > 1000 {
            return Err(Error::ValidationError("单次最多检查 1000 个目标".into()));
        }
        let mut targets = BTreeSet::new();
        for id in ids {
            if normalize_required_text_ref(id, "目标不能为空", 128, "目标过长")? != *id {
                return Err(Error::ValidationError("目标身份不能带首尾空白".into()));
            }
            targets.insert((*id).to_owned());
        }
        Ok(Self(targets))
    }
    /// 取得已校验的目标集合。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不可变、去重后的目标 ID。
    /// # 错误
    /// 不返回错误。
    pub fn ids(&self) -> &BTreeSet<String> {
        &self.0
    }
}

/// 只对本次目标有效的授权结果，禁止用于全量列表、计数或汇总。
#[derive(Debug, Clone)]
pub struct AuthorizedResourceAccess {
    pub(crate) query: AuthorizedResourceQuery,
    pub(crate) targets: ResourceTargets,
    pub(crate) ids: BTreeSet<String>,
}
impl AuthorizedResourceAccess {
    /// 取得本次目标授权版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原范围及账号资源授权的组合版本；使用结果前必须在当前事务重新解析。
    /// # 错误
    /// 不返回错误。
    pub fn scope_version(&self) -> &str {
        &self.query.scope_version
    }
    /// 使用与查询相同的并集规则检查单个对象。
    /// # 参数
    /// id、object 为可信对象身份及责任事实。
    /// # 返回
    /// 是否具备该对象读取资格。
    /// # 错误
    /// 不返回错误；不证明任务分配或业务状态。
    pub fn allows(&self, id: &str, object: &ScopedObject<'_>) -> bool {
        self.targets.ids().contains(id) && (self.query.scope.allows(object, true) || self.ids.contains(id))
    }
    /// 批量检查使用与单条相同的规则，不接收客户端预裁剪的完整操作集合。
    /// # 参数
    /// objects 为业务拥有领域读取的目标集合。
    /// # 返回
    /// 按输入顺序返回判定。
    /// # 错误
    /// 不返回错误。
    pub fn batch(&self, objects: &[(&str, ScopedObject<'_>)]) -> Vec<bool> {
        objects.iter().map(|(id, object)| self.allows(id, object)).collect()
    }
    /// 检查任务办理必须同时满足当前分配、可办理状态及全部必要来源。
    /// # 参数
    /// assigned、actionable 为当前可信任务事实；objects 为服务端完整来源集合。
    /// # 返回
    /// 所有条件满足时成功。
    /// # 错误
    /// 未分配、结束、空来源或任一来源无权时拒绝。
    pub fn ensure_task(
        &self,
        assigned: bool,
        actionable: bool,
        objects: &[(&str, ScopedObject<'_>)],
    ) -> Result<()> {
        if !assigned || !actionable || objects.is_empty() || self.batch(objects).contains(&false) {
            return Err(Error::Forbidden("任务分配、状态或来源读取资格不满足".into()));
        }
        Ok(())
    }
}

/// 原范围版本与账号资源修订号共同标识授权；不依赖枚举授权记录。
pub(crate) fn scope_version(base: &str, entry: &Permission, resource: ResourceKind, revision: u64) -> String {
    format!("{:x}", md5::compute(format!("{base}:{entry:?}:{resource:?}:{revision}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Permission;
    use crate::entity::access_control::ScopeClause;

    fn object(owned: bool) -> ScopedObject<'static> {
        ScopedObject {
            owned,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: None,
            settlement_party_id: None,
            warehouse_id: None,
        }
    }
    #[test]
    fn target_results_never_authorize_unrequested_records() {
        let access = AuthorizedResourceAccess {
            query: AuthorizedResourceQuery {
                user_id: "u".into(),
                resource: ResourceKind::SalesOrder,
                scope: ResolvedScope {
                    role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                    user_limit: None,
                },
                scope_version: "v".into(),
                grant_revision: 1,
            },
            targets: ResourceTargets::new(&["s1", "s2"]).unwrap(),
            ids: BTreeSet::from(["s1".into()]),
        };
        assert!(access.allows("s1", &object(false)));
        assert!(!access.allows("s2", &object(false)));
        assert!(access.allows("s2", &object(true)));
        assert!(!access.allows("unrequested", &object(true)));
        assert!(access.ensure_task(true, true, &[("s1", object(false)), ("s2", object(true))]).is_ok());
        assert!(access.ensure_task(true, true, &[("s1", object(false)), ("s2", object(false))]).is_err());
        assert!(access.ensure_task(false, true, &[("s1", object(false))]).is_err());
        assert!(access.ensure_task(true, false, &[("s1", object(false))]).is_err());
        assert!(access.ensure_task(true, true, &[]).is_err());
    }
    #[test]
    fn target_inputs_are_validated_and_deduplicated() {
        assert!(ResourceTargets::new(&[]).unwrap().ids().is_empty());
        assert_eq!(
            ResourceTargets::new(&["b", "a", "a"]).unwrap().ids(),
            &BTreeSet::from(["a".into(), "b".into()])
        );
        assert!(ResourceTargets::new(&[" "]).is_err());
        assert!(ResourceTargets::new(&[" a"]).is_err());
        assert!(ResourceTargets::new(&vec!["a"; 1001]).is_err());
    }
    #[test]
    fn scope_versions_bind_revision_resource_and_entry() {
        let entry = Permission::parse("invoice:create").unwrap();
        let version = scope_version("base", &entry, ResourceKind::SalesOrder, 0);
        assert_eq!(version, scope_version("base", &entry, ResourceKind::SalesOrder, 0));
        assert_ne!(version, scope_version("base", &entry, ResourceKind::SalesOrder, 1));
        assert_ne!(version, scope_version("other", &entry, ResourceKind::SalesOrder, 0));
        assert_ne!(version, scope_version("base", &entry, ResourceKind::PurchaseOrder, 0));
        assert_ne!(
            version,
            scope_version("base", &Permission::parse("invoice:list").unwrap(), ResourceKind::SalesOrder, 0)
        );
    }
}
