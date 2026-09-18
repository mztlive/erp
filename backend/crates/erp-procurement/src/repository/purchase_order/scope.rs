//! 采购单当前责任范围查询；输入必须由应用层完成资源动作授权。

use mongodb::bson::{Document, doc};
use persistence_core::QueryFilter;

/// 一个已经通过同角色权限证明的采购责任范围。
#[derive(Debug, Clone, Default)]
pub struct PurchaseScopeClause {
    /// 公司范围覆盖该资源动作的全部采购单。
    pub company: bool,
    /// 当前采购负责人为该账号的单据。
    pub owner_user_id: Option<String>,
    /// 单据业务组织必须属于的内部组织。
    pub business_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default)]
pub struct PurchaseReadScope {
    /// 其他资源动作的独立范围，全部必须同时满足。
    pub required_scopes: Vec<PurchaseReadScope>,
    /// 同角色正向范围。
    pub roles: Vec<PurchaseScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<PurchaseScopeClause>,
    /// 已证明合法读取参与的单据 ID，不得由历史归属快照构造。
    pub historical_order_ids: Vec<String>,
}

impl PurchaseScopeClause {
    /// 按与仓储条件相同的当前责任事实判断对象，不使用审计创建人。
    ///
    /// # 参数
    /// * `owner` - 当前或拟写入的采购负责人
    /// * `org` - 单据业务组织
    ///
    /// # 返回
    /// 公司、本人负责或业务组织任一命中即覆盖。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把仓库 ID 与部门 ID 放入同一并集；创建人不得充当负责人。
    #[cfg(test)]
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

    /// 将当前采购责任转换为固定字段条件；空范围明确无结果。
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
    /// 只解释 `owner_user_id` 与 `business_org_unit_id`，不得并入仓库字段。
    fn document(&self) -> Document {
        if self.company {
            return doc! {};
        }
        let mut conditions = Vec::new();
        if let Some(user) = &self.owner_user_id {
            conditions.push(doc! { "owner_user_id": user });
        }
        if !self.business_org_unit_ids.is_empty() {
            conditions.push(doc! { "business_org_unit_id": { "$in": &self.business_org_unit_ids } });
        }
        union(conditions)
    }
}

impl PurchaseReadScope {
    /// 校验尚未持久化的单据责任；只使用正向角色范围与个人上限。
    ///
    /// # 参数
    /// * `owner` - 拟写入的采购负责人
    /// * `org` - 拟写入的业务组织
    ///
    /// # 返回
    /// 全部独立资源范围同时覆盖责任事实时返回 true，历史参与不授予创建权。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 创建固定以当前采购负责人和业务组织解释，不得回退创建人。
    #[cfg(test)]
    pub fn allows_creation(&self, owner: &str, org: &str) -> bool {
        self.roles.iter().any(|clause| clause.allows(owner, org))
            && self.user_limit.as_ref().is_none_or(|clause| clause.allows(owner, org))
            && self.required_scopes.iter().all(|scope| scope.allows_creation(owner, org))
    }

    /// 判断完整采购集合是否已获授权，个人上限仍须允许公司范围。
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
        self.required_scopes.iter().all(Self::is_company)
            && self.roles.iter().any(|clause| clause.company)
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
        self.required_scopes.iter().any(Self::is_empty)
            || (self.roles.iter().all(PurchaseScopeClause::is_empty) && self.historical_order_ids.is_empty())
            || self.user_limit.as_ref().is_some_and(PurchaseScopeClause::is_empty)
    }

    /// 生成角色并集与个人上限交集，仓库字段不参与授权。
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
    /// 历史参与只补充读取，且继续受个人上限约束。
    pub fn document(&self) -> Document {
        let mut grants = self.roles.iter().map(PurchaseScopeClause::document).collect::<Vec<_>>();
        if !self.historical_order_ids.is_empty() {
            grants.push(doc! { "id": { "$in": &self.historical_order_ids } });
        }
        let mut roles = union(grants);
        if !self.required_scopes.is_empty() {
            let mut required = self.required_scopes.iter().map(Self::document).collect::<Vec<_>>();
            required.push(roles);
            roles = doc! { "$and": required };
        }
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }
}

/// 采购主表授权范围查询。
#[allow(async_fn_in_trait)]
pub trait PurchaseOrderRepositoryScopeExt {
    /// 按独立授权条件读取采购单，ID 条件不能替换范围交集。
    ///
    /// # 参数
    /// * `id` - 采购单稳定主键
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 None。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 仓储不得按登录用户自行推断权限。
    async fn find_authorized(
        &self,
        id: &str,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<crate::entity::purchase_order::PurchaseOrder>>;

    /// 读取范围内采购单主键，供变更单和退货沿来源单过滤。
    ///
    /// # 参数
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 个主键；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 仓储不得按登录用户自行推断权限；公司范围应由调用方跳过本方法。
    async fn list_authorized_ids(
        &self,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<String>>;

    /// 装载查询的有界身份与版本集合，用于跨页及导出的一致性校验。
    ///
    /// # 参数
    /// * `filter` - 业务筛选
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行；调用方必须整体拒绝超限，不得截断版本集合。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 版本集合必须与列表同一授权条件和筛选快照。
    async fn query_versions(
        &self,
        filter: &super::PurchaseOrderFilter,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<PurchaseVersion>>;
}

impl PurchaseOrderRepositoryScopeExt
    for persistence_core::Repository<'_, crate::entity::purchase_order::PurchaseOrder>
{
    async fn find_authorized(
        &self,
        id: &str,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Option<crate::entity::purchase_order::PurchaseOrder>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor).await
    }

    async fn list_authorized_ids(
        &self,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<String>> {
        let versions = persistence_core::mongo_ops::find_many(
            &self.collection().clone_with_type::<PurchaseVersion>(),
            doc! {
                "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
                "$and": [scope.document()]
            },
            mongodb::options::FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            executor,
        )
        .await?;
        Ok(versions.into_iter().map(|row| row.id).collect())
    }

    async fn query_versions(
        &self,
        filter: &super::PurchaseOrderFilter,
        scope: &PurchaseReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<Vec<PurchaseVersion>> {
        persistence_core::mongo_ops::find_many(
            &self.collection().clone_with_type::<PurchaseVersion>(),
            doc! { "$and": [filter.to_doc(), scope.document()] },
            mongodb::options::FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            executor,
        )
        .await
    }
}

/// MongoDB 不接受空 $or；用恒假表达式保持缺范围拒绝语义。
///
/// # 参数
/// * `conditions` - 角色或条款并集
///
/// # 返回
/// 空集合返回恒假条件，否则返回 `$or`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 缺范围不得解释为匹配全部单据。
fn union(conditions: Vec<Document>) -> Document {
    if conditions.is_empty() {
        return doc! { "$expr": false };
    }
    doc! { "$or": conditions }
}

/// 跨页校验使用的业务身份和版本，不包含金额及其他展示字段。
#[derive(Debug, serde::Deserialize, Hash)]
pub struct PurchaseVersion {
    /// 采购单稳定主键。
    pub id: String,
    /// 采购单乐观锁版本。
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_requires_role_scope_and_every_action_and_personal_limit() {
        let mut scope = PurchaseReadScope {
            roles: vec![PurchaseScopeClause { company: true, ..Default::default() }],
            required_scopes: vec![PurchaseReadScope {
                roles: vec![PurchaseScopeClause {
                    owner_user_id: Some("buyer-a".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(scope.allows_creation("buyer-a", "org-a"));
        assert!(!scope.allows_creation("buyer-b", "org-a"));
        scope.user_limit =
            Some(PurchaseScopeClause { business_org_unit_ids: vec!["org-b".into()], ..Default::default() });
        assert!(!scope.allows_creation("buyer-a", "org-a"));
        assert!(scope.allows_creation("buyer-a", "org-b"));
        scope.roles.clear();
        scope.historical_order_ids.push("old-order".into());
        assert!(!scope.allows_creation("buyer-a", "org-b"));
    }

    #[test]
    fn missing_scope_and_company_with_empty_personal_limit_stay_restricted() {
        assert_eq!(PurchaseReadScope::default().document(), doc! { "$expr": false });
        assert!(PurchaseReadScope::default().is_empty());
        let scope = PurchaseReadScope {
            required_scopes: vec![],
            roles: vec![PurchaseScopeClause { company: true, ..Default::default() }],
            user_limit: Some(PurchaseScopeClause::default()),
            historical_order_ids: vec![],
        };
        assert_eq!(scope.document(), doc! { "$and": [{ "$or": [{}] }, { "$expr": false }] });
        assert!(scope.is_empty());
    }

    #[test]
    fn historical_participation_adds_reads_without_bypassing_personal_limit() {
        let mut scope =
            PurchaseReadScope { historical_order_ids: vec!["old-order".into()], ..Default::default() };
        assert!(!scope.is_empty());
        assert_eq!(scope.document(), doc! { "$or": [{ "id": { "$in": ["old-order"] } }] });
        scope.user_limit = Some(PurchaseScopeClause::default());
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
        let scope = PurchaseReadScope {
            required_scopes: vec![],
            roles: vec![PurchaseScopeClause {
                owner_user_id: Some("buyer-a".into()),
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
                { "owner_user_id": "buyer-a" },
                { "business_org_unit_id": { "$in": ["org-a"] } }
            ] }] }
        );
    }

    #[test]
    fn warehouse_ids_are_not_unioned_with_business_org_conditions() {
        let document = PurchaseScopeClause {
            owner_user_id: Some("buyer-a".into()),
            business_org_unit_ids: vec!["org-a".into()],
            ..Default::default()
        }
        .document();
        let encoded = document.to_string();
        assert!(!encoded.contains("target_warehouse_id"));
        assert!(!encoded.contains("warehouse"));
        assert!(encoded.contains("business_org_unit_id"));
        assert!(encoded.contains("owner_user_id"));
    }
}
