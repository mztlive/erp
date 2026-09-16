//! 选品册与方案当前责任范围查询；输入必须由应用层完成资源动作授权。

use mongodb::bson::{Document, doc};

///
/// 一个已经通过同角色权限证明的选品责任范围。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionScopeClause {
    /// 公司范围覆盖该资源动作的全部对象。
    pub company: bool,
    /// 当前销售负责人为该账号的对象。
    pub owner_user_id: Option<String>,
    /// 业务组织必须属于的内部组织。
    pub business_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionReadScope {
    /// 同角色正向范围。
    pub roles: Vec<SelectionScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<SelectionScopeClause>,
    /// 已证明合法读取参与的对象 ID，不得由历史归属快照构造。
    pub historical_ids: Vec<String>,
}

impl SelectionScopeClause {
    /// 按与仓储条件相同的当前责任事实判断对象，不使用创建人或提交人。
    ///
    /// # 参数
    /// * `owner` - 当前或拟写入的销售负责人
    /// * `org` - 业务组织
    ///
    /// # 返回
    /// 公司、本人负责或业务组织任一命中即覆盖。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把创建人、提交人或客户联系人当作负责人。
    fn allows(&self, owner: &str, org: &str) -> bool {
        self.company
            || self.owner_user_id.as_deref() == Some(owner)
            || self.business_org_unit_ids.iter().any(|id| id == org)
    }

    /// 没有任何可匹配责任条件时，范围确定为空。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 非公司且没有负责人或组织条件时为空。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 空条款必须保持空集，不得补公司范围。
    fn is_empty(&self) -> bool {
        !self.company && self.owner_user_id.is_none() && self.business_org_unit_ids.is_empty()
    }

    /// 将当前选品责任转换为固定字段条件；空范围明确无结果。
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
    /// 只解释 `sales_owner_user_id` 与 `business_org_unit_id`。
    fn document(&self) -> Document {
        if self.company {
            return doc! {};
        }
        let mut conditions = Vec::new();
        if let Some(user) = &self.owner_user_id {
            conditions.push(doc! { "sales_owner_user_id": user });
        }
        if !self.business_org_unit_ids.is_empty() {
            conditions.push(doc! { "business_org_unit_id": { "$in": &self.business_org_unit_ids } });
        }
        super::scope_union(conditions)
    }
}

impl SelectionReadScope {
    /// 判断完整选品集合是否已获授权，个人上限仍须允许公司范围。
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
        (self.roles.iter().all(SelectionScopeClause::is_empty) && self.historical_ids.is_empty())
            || self.user_limit.as_ref().is_some_and(SelectionScopeClause::is_empty)
    }

    /// 生成角色并集与个人上限交集，历史参与只补充读取。
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
    /// 历史参与继续受个人上限约束。
    pub fn document(&self) -> Document {
        let mut grants = self.roles.iter().map(SelectionScopeClause::document).collect::<Vec<_>>();
        if !self.historical_ids.is_empty() {
            grants.push(doc! { "id": { "$in": &self.historical_ids } });
        }
        let roles = super::scope_union(grants);
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }

    /// 按与 [`SelectionReadScope::document`] 相同的责任事实判断单对象。
    ///
    /// # 参数
    /// * `owner` - 对象显式销售负责人
    /// * `org` - 对象业务组织
    ///
    /// # 返回
    /// 角色并集命中且个人上限允许时为 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只解释显式负责人与业务组织；创建人、提交人不构成命中。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responsibility_uses_explicit_owner_and_org() {
        let scope = SelectionReadScope {
            roles: vec![SelectionScopeClause {
                owner_user_id: Some("sales-a".into()),
                business_org_unit_ids: vec!["org-a".into()],
                ..Default::default()
            }],
            user_limit: None,
            historical_ids: vec![],
        };
        assert!(!scope.is_empty());
        assert_eq!(
            scope.document(),
            mongodb::bson::doc! { "$or": [{ "$or": [
                { "sales_owner_user_id": "sales-a" },
                { "business_org_unit_id": { "$in": ["org-a"] } }
            ] }] }
        );
    }

    #[test]
    fn missing_scope_stays_restricted() {
        assert_eq!(SelectionReadScope::default().document(), mongodb::bson::doc! { "$expr": false });
        assert!(SelectionReadScope::default().is_empty());
    }

    #[test]
    fn submitter_identity_does_not_grant_ownership() {
        let clause = SelectionScopeClause { owner_user_id: Some("sales-a".into()), ..Default::default() };
        assert!(clause.allows("sales-a", "org-a"));
        assert!(!clause.allows("submitter", "org-a"));
    }
}
