//! 销售单当前责任范围查询；输入必须由应用层完成资源动作授权。

use mongodb::bson::{doc, Document};

/// 一个已经通过同角色权限证明的销售责任范围。
#[derive(Debug, Clone, Default)]
pub struct SalesScopeClause {
    pub company: bool,
    pub owner_user_id: Option<String>,
    pub business_org_unit_ids: Vec<String>,
    pub collaborative_customer_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default)]
pub struct SalesReadScope {
    pub roles: Vec<SalesScopeClause>,
    pub user_limit: Option<SalesScopeClause>,
    /// 已证明合法读取参与的单据 ID，不得由历史归属快照构造。
    pub historical_order_ids: Vec<String>,
}

impl SalesScopeClause {
    /// 没有任何可匹配责任条件时，范围确定为空。
    fn is_empty(&self) -> bool {
        !self.company
            && self.owner_user_id.is_none()
            && self.business_org_unit_ids.is_empty()
            && self.collaborative_customer_ids.is_empty()
    }
    /// 将当前销售责任转换为固定字段条件；空范围明确无结果。
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
        if !self.collaborative_customer_ids.is_empty() {
            conditions.push(doc! { "customer_id": { "$in": &self.collaborative_customer_ids } });
        }
        union(conditions)
    }
}

impl SalesReadScope {
    /// 判断授权规则是否确定不产生可见对象。
    ///
    /// # 返回
    /// 仅证明空授权；非空规则仍可能因期间或业务筛选而没有记录。
    pub fn is_empty(&self) -> bool {
        (self.roles.iter().all(SalesScopeClause::is_empty) && self.historical_order_ids.is_empty())
            || self.user_limit.as_ref().is_some_and(SalesScopeClause::is_empty)
    }
    /// 生成角色并集与个人上限交集，历史业绩字段不参与授权。
    ///
    /// # 返回
    /// 返回仓储读取条件，空角色集不产生全量授权。
    pub fn document(&self) -> Document {
        let mut grants = self
            .roles
            .iter()
            .map(SalesScopeClause::document)
            .collect::<Vec<_>>();
        if !self.historical_order_ids.is_empty() {
            grants.push(doc! { "id": { "$in": &self.historical_order_ids } });
        }
        let roles = union(grants);
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }
}

/// MongoDB 不接受空 $or；用恒假表达式保持缺范围拒绝语义。
fn union(conditions: Vec<Document>) -> Document {
    if conditions.is_empty() {
        return doc! { "$expr": false };
    }
    doc! { "$or": conditions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_scope_and_company_with_empty_personal_limit_stay_restricted() {
        assert_eq!(SalesReadScope::default().document(), doc! { "$expr": false });
        assert!(SalesReadScope::default().is_empty());
        let scope = SalesReadScope {
            roles: vec![SalesScopeClause {
                company: true,
                ..Default::default()
            }],
            user_limit: Some(SalesScopeClause::default()),
            historical_order_ids: vec![],
        };
        assert_eq!(
            scope.document(),
            doc! { "$and": [{ "$or": [{}] }, { "$expr": false }] }
        );
        assert!(scope.is_empty());
    }

    #[test]
    fn historical_participation_adds_reads_without_bypassing_personal_limit() {
        let mut scope = SalesReadScope {
            historical_order_ids: vec!["old-order".into()],
            ..Default::default()
        };
        assert!(!scope.is_empty());
        assert_eq!(
            scope.document(),
            doc! { "$or": [{ "id": { "$in": ["old-order"] } }] }
        );
        scope.user_limit = Some(SalesScopeClause::default());
        assert!(scope.is_empty());
        assert_eq!(
            scope.document(),
            doc! { "$and": [
                { "$or": [{ "id": { "$in": ["old-order"] } }] }, { "$expr": false }
            ] }
        );
    }

    #[test]
    fn responsibility_uses_current_order_facts_and_keeps_role_alternatives() {
        let scope = SalesReadScope {
            roles: vec![SalesScopeClause {
                owner_user_id: Some("sales-a".into()),
                business_org_unit_ids: vec!["org-a".into()],
                ..Default::default()
            }],
            user_limit: None,
            historical_order_ids: vec![],
        };
        assert!(!scope.is_empty());
        assert_eq!(
            scope.document(),
            doc! { "$or": [{ "$or": [
                { "sales_owner_user_id": "sales-a" },
                { "business_org_unit_id": { "$in": ["org-a"] } }
            ] }] }
        );
    }
}
