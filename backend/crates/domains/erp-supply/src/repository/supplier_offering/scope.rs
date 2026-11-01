//! 供给当前维护责任范围查询；输入必须由应用层完成资源动作授权。

use mongodb::bson::{Document, doc};

/// 一个已经通过同角色权限证明的供给责任范围。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OfferingScopeClause {
    /// 公司范围覆盖该资源动作的全部供给。
    pub company: bool,
    /// 当前维护人为该账号的供给。
    pub owner_user_id: Option<String>,
    /// 业务组织必须属于的内部组织。
    pub business_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OfferingReadScope {
    /// 同角色正向范围。
    pub roles: Vec<OfferingScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<OfferingScopeClause>,
}

impl OfferingScopeClause {
    /// 按与仓储条件相同的当前责任事实判断对象，不使用创建人。
    ///
    /// # 参数
    /// * `owner` - 当前或拟写入的维护人
    /// * `org` - 业务组织
    ///
    /// # 返回
    /// 公司、本人负责或业务组织任一命中即覆盖。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把创建人、采购负责人或供应商联系人当作维护人。
    pub fn allows(&self, owner: &str, org: &str) -> bool {
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

    /// 将当前供给责任转换为固定字段条件；空范围明确无结果。
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
    /// 只解释 `maintainer_user_id` 与 `business_org_unit_id`。
    fn document(&self) -> Document {
        if self.company {
            return doc! {};
        }
        let mut conditions = Vec::new();
        if let Some(user) = &self.owner_user_id {
            conditions.push(doc! { "maintainer_user_id": user });
        }
        if !self.business_org_unit_ids.is_empty() {
            conditions.push(doc! { "business_org_unit_id": { "$in": &self.business_org_unit_ids } });
        }
        scope_union(conditions)
    }
}

impl OfferingReadScope {
    /// 判断完整供给集合是否已获授权，个人上限仍须允许公司范围。
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
        self.roles.iter().all(OfferingScopeClause::is_empty)
            || self.user_limit.as_ref().is_some_and(OfferingScopeClause::is_empty)
    }

    /// 生成角色并集与个人上限交集；供给不允许历史参与。
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
    /// 采购负责人筛选不得写入本条件。
    pub fn document(&self) -> Document {
        let grants = self.roles.iter().map(OfferingScopeClause::document).collect::<Vec<_>>();
        let roles = scope_union(grants);
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }

    /// 按与 [`OfferingReadScope::document`] 相同的责任事实判断单对象。
    ///
    /// # 参数
    /// * `owner` - 对象显式维护人
    /// * `org` - 对象业务组织
    ///
    /// # 返回
    /// 角色并集命中且个人上限允许时为 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只解释显式维护人与业务组织；创建人、采购负责人不构成命中。
    pub fn allows_object(&self, owner: &str, org: &str) -> bool {
        self.roles.iter().any(|clause| clause.allows(owner, org))
            && self.user_limit.as_ref().is_none_or(|clause| clause.allows(owner, org))
    }
}

/// 返回 `$or` 或恒假条件。
///
/// # 参数
/// * `conditions` - 同一角色内可并列的责任条件
///
/// # 返回
/// 空集合返回恒假表达式，避免 Mongo 空 `$or`。
///
/// # 错误
/// 无。
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
        let scope = OfferingReadScope::default();
        assert!(scope.is_empty());
        assert!(!scope.allows_object("user-1", "org-1"));
        assert_eq!(scope.document(), doc! { "$expr": false });
    }

    #[test]
    fn company_and_self_owned_keep_union_and_user_limit_intersection() {
        let scope = OfferingReadScope {
            roles: vec![
                OfferingScopeClause { company: true, ..OfferingScopeClause::default() },
                OfferingScopeClause {
                    owner_user_id: Some("user-1".into()),
                    ..OfferingScopeClause::default()
                },
            ],
            user_limit: Some(OfferingScopeClause {
                business_org_unit_ids: vec!["org-a".into()],
                ..OfferingScopeClause::default()
            }),
        };
        assert!(scope.allows_object("user-1", "org-a"));
        assert!(!scope.allows_object("user-1", "org-b"));
        assert_eq!(
            scope.document(),
            doc! {
                "$and": [
                    { "$or": [ {}, { "$or": [ { "maintainer_user_id": "user-1" } ] } ] },
                    { "$or": [ { "business_org_unit_id": { "$in": ["org-a"] } } ] },
                ]
            }
        );
    }
}
