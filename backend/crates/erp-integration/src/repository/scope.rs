//! 集成授权条件；由应用层完成 DataScope 解析后再交给仓储。

use mongodb::bson::{Document, doc};

/// 一个已经通过同角色权限证明的集成责任范围。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationScopeClause {
    /// 公司范围覆盖该资源动作的全部对象。
    pub company: bool,
    /// 当前处理人为该账号的对象。
    pub owner_user_id: Option<String>,
    /// 当前处理人必须属于的内部组织。
    pub owner_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationReadScope {
    /// 同角色正向范围。
    pub roles: Vec<IntegrationScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<IntegrationScopeClause>,
}

impl IntegrationScopeClause {
    /// 按与仓储条件相同的当前处理人事实判断对象。
    ///
    /// # 参数
    /// * `owner` - 当前处理人
    /// * `org` - 当前处理人所属内部组织
    ///
    /// # 返回
    /// 公司、本人处理或处理人组织任一命中即覆盖。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把创建人、系统账号或历史处理人当作当前处理人。
    pub fn allows(&self, owner: &str, org: &str) -> bool {
        self.company
            || self.owner_user_id.as_deref() == Some(owner)
            || self.owner_org_unit_ids.iter().any(|id| id == org)
    }

    /// 没有任何可匹配责任条件时，范围确定为空（与 Port 侧 `has_scope_rules` 互为否定）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 非公司且没有处理人或组织条件时为空（即 Port 侧 `has_scope_rules() == false`）。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 空条款必须保持空集，不得补公司范围。
    pub fn is_empty(&self) -> bool {
        !self.has_scope_rules()
    }

    /// 判断条款是否构成有效责任条件（与 [`is_empty`](Self::is_empty) 互为否定）。
    ///
    /// # 返回
    /// 公司、本人处理或处理人组织任一命中条件存在时为 true。
    pub fn has_scope_rules(&self) -> bool {
        self.company || self.owner_user_id.is_some() || !self.owner_org_unit_ids.is_empty()
    }

    /// 将当前处理人责任转换为固定字段条件；空范围明确无结果。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回仓储读取条件。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只解释 `owner_user_id` 与 `owner_org_unit_id`。
    pub fn document(&self) -> Document {
        if self.company {
            return doc! {};
        }
        let mut conditions = Vec::new();
        if let Some(user) = &self.owner_user_id {
            conditions.push(doc! { "owner_user_id": user });
        }
        if !self.owner_org_unit_ids.is_empty() {
            conditions.push(doc! { "owner_org_unit_id": { "$in": &self.owner_org_unit_ids } });
        }
        scope_union(conditions)
    }
}

impl IntegrationReadScope {
    /// 判断完整集合是否已获授权，个人上限仍须允许公司范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅在角色公司授权且未被个人上限收窄时返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 公司范围仍受个人上限约束，不得用其补齐缺范围角色。
    pub fn is_company(&self) -> bool {
        self.roles.iter().any(|clause| clause.company)
            && self.user_limit.as_ref().is_none_or(|clause| clause.company)
    }

    /// 判断授权规则是否确定不产生可见对象。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅证明空授权；非空规则仍可能因业务筛选而没有记录。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 缺范围保持空集，不得退化为全量查询。
    pub fn is_empty(&self) -> bool {
        self.roles.iter().all(IntegrationScopeClause::is_empty)
            || self.user_limit.as_ref().is_some_and(IntegrationScopeClause::is_empty)
    }

    /// 生成角色并集与个人上限交集；历史处理人不进入授权并集。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回仓储读取条件，空角色集不产生全量授权。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 历史处理人只作筛选，不授写、不扩大正向范围。
    pub fn document(&self) -> Document {
        if self.is_empty() {
            return empty_ids();
        }
        let grants = self.roles.iter().map(IntegrationScopeClause::document).collect::<Vec<_>>();
        let roles = scope_union(grants);
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }

    /// 按与 [`IntegrationReadScope::document`] 相同的责任事实判断单对象。
    ///
    /// # 参数
    /// * `owner` - 对象当前处理人
    /// * `org` - 处理人所属内部组织
    ///
    /// # 返回
    /// 角色并集命中且个人上限允许时为 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只解释当前处理人与其内部组织；历史处理人不构成命中。
    pub fn allows_object(&self, owner: &str, org: &str) -> bool {
        if self.is_company() {
            return true;
        }
        if self.is_empty() {
            return false;
        }
        let roles_allow = self.roles.iter().any(|clause| clause.allows(owner, org));
        let limit_allows = self.user_limit.as_ref().is_none_or(|clause| clause.allows(owner, org));
        roles_allow && limit_allows
    }
}

fn empty_ids() -> Document {
    doc! { "id": { "$in": Vec::<String>::new() } }
}

/// 将同角色条款按并集组合；没有任何条款时明确无结果。
fn scope_union(conditions: Vec<Document>) -> Document {
    match conditions.len() {
        0 => empty_ids(),
        1 => conditions.into_iter().next().expect("len==1"),
        _ => doc! { "$or": conditions },
    }
}

#[cfg(test)]
mod scope_parity_tests {
    use mongodb::bson::doc;

    use super::*;
    use crate::ports::IntegrationResolvedClause;

    /// Port 侧条款与仓储侧条款的内存判定对拍（同一事实同一结论）。
    #[test]
    fn memory_decision_matches_across_layers() {
        let cases = [
            (IntegrationResolvedClause { company: true, ..Default::default() }, true),
            (
                IntegrationResolvedClause { self_owned: true, ..Default::default() },
                // 仓储侧需要具体账号才能判定本人命中；此处只对拍"非空"口径。
                true,
            ),
            (IntegrationResolvedClause { org_unit_ids: vec!["org-a".into()], ..Default::default() }, true),
            (IntegrationResolvedClause::default(), false),
        ];
        for (clause, has_rules) in cases {
            assert_eq!(clause.has_scope_rules(), has_rules, "Port 侧判定漂移");
            assert_eq!(!clause.is_empty(), has_rules, "Port 侧正反语义不对称");
            let repo = IntegrationScopeClause {
                company: clause.company,
                owner_user_id: clause.self_owned.then(|| "user-1".to_string()),
                owner_org_unit_ids: clause.org_unit_ids.clone(),
            };
            assert_eq!(repo.has_scope_rules(), has_rules, "仓储侧判定漂移");
            assert_eq!(!repo.is_empty(), has_rules, "仓储侧正反语义不对称");
        }
    }

    #[test]
    fn empty_scope_compiles_to_empty_id_set() {
        let scope = IntegrationReadScope::default();
        assert!(scope.is_empty());
        assert_eq!(scope.document(), empty_ids());
        assert!(!scope.allows_object("user-1", "org-a"));
    }

    #[test]
    fn company_scope_is_unrestricted_until_user_limit() {
        let scope = IntegrationReadScope {
            roles: vec![IntegrationScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        };
        assert!(scope.is_company());
        assert_eq!(scope.document(), doc! {});
        assert!(scope.allows_object("user-1", "org-a"));
    }
}
