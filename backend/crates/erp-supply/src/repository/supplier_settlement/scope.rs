//! 结算授权条件；由应用层完成 DataScope 解析后再交给仓储。

use mongodb::bson::{Document, doc};

/// 一个已经通过同角色权限证明的结算责任范围。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettlementScopeClause {
    /// 公司范围覆盖该资源动作的全部结算单。
    pub company: bool,
    /// 当前对账负责人为该账号的对象。
    pub owner_user_id: Option<String>,
    /// 业务组织必须属于的内部组织。
    pub business_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettlementReadScope {
    /// 同角色正向范围。
    pub roles: Vec<SettlementScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<SettlementScopeClause>,
}

impl SettlementScopeClause {
    /// 按与仓储条件相同的当前责任事实判断对象，不使用创建人。
    ///
    /// # 参数
    /// * `owner` - 当前或拟写入的对账负责人
    /// * `org` - 业务组织
    ///
    /// # 返回
    /// 公司、本人负责或业务组织任一命中即覆盖。
    ///
    /// # 关键业务约束
    /// 不得把创建人、差异处理人或复核人当作对账负责人。
    pub fn allows(&self, owner: &str, org: &str) -> bool {
        self.company
            || self.owner_user_id.as_deref() == Some(owner)
            || self.business_org_unit_ids.iter().any(|id| id == org)
    }

    /// 没有任何可匹配责任条件时，范围确定为空。
    fn is_empty(&self) -> bool {
        !self.company && self.owner_user_id.is_none() && self.business_org_unit_ids.is_empty()
    }

    /// 将当前结算责任转换为固定字段条件；空范围明确无结果。
    fn document(&self) -> Document {
        if self.company {
            return doc! {};
        }
        let mut conditions = Vec::new();
        if let Some(user) = &self.owner_user_id {
            conditions.push(doc! { "prepared_by": user });
        }
        if !self.business_org_unit_ids.is_empty() {
            conditions.push(doc! { "business_org_unit_id": { "$in": &self.business_org_unit_ids } });
        }
        scope_union(conditions)
    }
}

impl SettlementReadScope {
    /// 判断完整结算集合是否已获授权，个人上限仍须允许公司范围。
    ///
    /// # 返回
    /// 仅在角色公司授权且未被个人上限收窄时返回 true。
    pub fn is_company(&self) -> bool {
        self.roles.iter().any(|clause| clause.company)
            && self.user_limit.as_ref().is_none_or(|clause| clause.company)
    }

    /// 判断授权规则是否确定不产生可见对象。
    ///
    /// # 返回
    /// 仅证明空授权；非空规则仍可能因业务筛选而没有记录。
    pub fn is_empty(&self) -> bool {
        self.roles.iter().all(SettlementScopeClause::is_empty)
            || self.user_limit.as_ref().is_some_and(SettlementScopeClause::is_empty)
    }

    /// 生成角色并集与个人上限交集。
    ///
    /// # 返回
    /// 返回仓储读取条件，空角色集不产生全量授权。
    ///
    /// # 关键业务约束
    /// 历史参与不允许；差异处理人与复核人筛选不得写入本条件。
    pub fn document(&self) -> Document {
        if self.is_company() {
            return doc! {};
        }
        let grants = self.roles.iter().map(SettlementScopeClause::document).collect::<Vec<_>>();
        let roles = scope_union(grants);
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }

    /// 按与 [`SettlementReadScope::document`] 相同的责任事实判断单对象。
    ///
    /// # 参数
    /// * `owner` - 对象对账负责人
    /// * `org` - 对象业务组织
    ///
    /// # 返回
    /// 角色并集命中且个人上限允许时为 true。
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

/// 返回 `$or` 或恒假条件。
fn scope_union(conditions: Vec<Document>) -> Document {
    if conditions.is_empty() {
        return doc! { "$expr": false };
    }
    doc! { "$or": conditions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scope_is_false_document_and_rejects_objects() {
        let scope = SettlementReadScope::default();
        assert!(scope.is_empty());
        assert!(!scope.allows_object("user-1", "org-1"));
        assert_eq!(scope.document(), doc! { "$expr": false });
    }

    #[test]
    fn self_owned_matches_prepared_by_not_created_by() {
        let scope = SettlementReadScope {
            roles: vec![SettlementScopeClause {
                owner_user_id: Some("owner".into()),
                ..SettlementScopeClause::default()
            }],
            user_limit: None,
        };
        assert!(scope.allows_object("owner", "org-x"));
        assert!(!scope.allows_object("created-by", "org-x"));
        assert_eq!(scope.document(), doc! { "$or": [doc! { "$or": [doc! { "prepared_by": "owner" }] }] });
    }

    #[test]
    fn three_role_filters_do_not_substitute_owner_authorization() {
        let owner =
            SettlementScopeClause { owner_user_id: Some("owner".into()), ..SettlementScopeClause::default() };
        assert!(owner.allows("owner", "org-a"));
        assert!(!owner.allows("operator", "org-a"));
        assert!(!owner.allows("reviewer", "org-a"));
        let org = SettlementScopeClause {
            business_org_unit_ids: vec!["org-a".into()],
            ..SettlementScopeClause::default()
        };
        assert!(org.allows("operator", "org-a"));
        assert!(!org.allows("operator", "org-b"));
    }
}
