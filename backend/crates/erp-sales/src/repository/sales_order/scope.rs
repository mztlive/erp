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
    /// 其他敏感资源的独立范围，全部必须同时满足。
    pub required_scopes: Vec<SalesReadScope>,
    pub roles: Vec<SalesScopeClause>,
    pub user_limit: Option<SalesScopeClause>,
    /// 已证明合法读取参与的单据 ID，不得由历史归属快照构造。
    pub historical_order_ids: Vec<String>,
}

impl SalesScopeClause {
    /// 按与仓储条件相同的当前责任事实判断待创建对象，不使用审计创建人。
    fn allows(&self, owner: &str, org: &str, customer: &str) -> bool {
        self.company
            || self.owner_user_id.as_deref() == Some(owner)
            || self.business_org_unit_ids.iter().any(|id| id == org)
            || self.collaborative_customer_ids.iter().any(|id| id == customer)
    }
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
    /// 校验尚未持久化的单据责任；只使用正向角色范围与个人上限。
    ///
    /// # 返回
    /// 全部独立资源范围同时覆盖责任事实时返回 true，历史参与不授予创建权。
    pub fn allows_creation(&self, owner: &str, org: &str, customer: &str) -> bool {
        self.roles.iter().any(|c| c.allows(owner, org, customer))
            && self
                .user_limit
                .as_ref()
                .is_none_or(|c| c.allows(owner, org, customer))
            && self
                .required_scopes
                .iter()
                .all(|c| c.allows_creation(owner, org, customer))
    }
    /// 判断完整销售集合是否已获授权，个人上限仍须允许公司范围。
    ///
    /// # 返回
    /// 仅在角色公司授权且未被个人上限收窄时返回 true。
    pub fn is_company(&self) -> bool {
        self.required_scopes.iter().all(Self::is_company)
            && self.roles.iter().any(|c| c.company)
            && self.user_limit.as_ref().is_none_or(|c| c.company)
    }
    /// 判断授权规则是否确定不产生可见对象。
    ///
    /// # 返回
    /// 仅证明空授权；非空规则仍可能因期间或业务筛选而没有记录。
    pub fn is_empty(&self) -> bool {
        self.required_scopes.iter().any(Self::is_empty)
            || (self.roles.iter().all(SalesScopeClause::is_empty) && self.historical_order_ids.is_empty())
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
        let mut roles = union(grants);
        if !self.required_scopes.is_empty() {
            let mut required = self
                .required_scopes
                .iter()
                .map(Self::document)
                .collect::<Vec<_>>();
            required.push(roles);
            roles = doc! { "$and": required };
        }
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }
}

impl super::super::owned::SalesOrderRepository<'_> {
    /// 按两组独立授权交集批量读取关联销售责任事实。
    ///
    /// # 返回
    /// 仅返回指定 ID 内同时满足两组范围的订单；调用方负责分批和总上限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    pub async fn scope_orders(
        &self,
        ids: &[String],
        sales: &SalesReadScope,
        resource: &SalesReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<crate::entity::sales_order::SalesOrder>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        self.find_many(
            doc! { "$and": [
                { "id": { "$in": ids } }, sales.document(), resource.document()
            ] },
            executor,
        )
        .await
    }
    /// 按独立授权条件读取销售单，ID 条件不能替换范围交集。
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 None。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    pub async fn find_authorized(
        &self,
        id: &str,
        scope: &SalesReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<crate::entity::sales_order::SalesOrder>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor)
            .await
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
    fn creation_requires_role_scope_and_every_action_and_personal_limit() {
        let mut scope = SalesReadScope {
            roles: vec![SalesScopeClause {
                company: true,
                ..Default::default()
            }],
            required_scopes: vec![SalesReadScope {
                roles: vec![SalesScopeClause {
                    owner_user_id: Some("sales-a".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(scope.allows_creation("sales-a", "org-a", "customer-a"));
        assert!(!scope.allows_creation("sales-b", "org-a", "customer-a"));
        scope.user_limit = Some(SalesScopeClause {
            business_org_unit_ids: vec!["org-b".into()],
            ..Default::default()
        });
        assert!(!scope.allows_creation("sales-a", "org-a", "customer-a"));
        assert!(scope.allows_creation("sales-a", "org-b", "customer-a"));
        scope.roles.clear();
        scope.historical_order_ids.push("old-order".into());
        assert!(!scope.allows_creation("sales-a", "org-b", "customer-a"));
    }

    #[test]
    fn independent_cost_scope_cannot_expand_sales_or_bypass_a_personal_limit() {
        let mut scope = SalesReadScope {
            roles: vec![SalesScopeClause {
                company: true,
                ..Default::default()
            }],
            required_scopes: vec![SalesReadScope {
                roles: vec![SalesScopeClause {
                    owner_user_id: Some("sales-a".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(!scope.is_company());
        assert!(!scope.is_empty());
        assert!(scope.document().contains_key("$and"));
        scope.required_scopes[0].user_limit = Some(SalesScopeClause::default());
        assert!(scope.is_empty());
        assert!(!scope.is_company());
    }

    #[test]
    fn missing_scope_and_company_with_empty_personal_limit_stay_restricted() {
        assert_eq!(SalesReadScope::default().document(), doc! { "$expr": false });
        assert!(SalesReadScope::default().is_empty());
        let scope = SalesReadScope {
            required_scopes: vec![],
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
            required_scopes: vec![],
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

/// 跨页校验使用的业务身份和版本，不包含金额及其他展示字段。
#[derive(Debug, serde::Deserialize, Hash)]
pub struct SalesVersion {
    pub id: String,
    pub version: u64,
}
