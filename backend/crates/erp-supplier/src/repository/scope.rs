//! 供应商授权条件；由应用层完成 DataScope 解析后再交给仓储。

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Result, mongo_ops};
use serde::Deserialize;

use super::owned::SupplierAccountRepository;
use super::supplier::SupplierAccountFilter;
use crate::entity::supplier::SupplierAccount;

/// 一个已经通过同角色权限证明的供应商责任范围。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplierScopeClause {
    /// 公司范围覆盖该资源动作的全部供应商。
    pub company: bool,
    /// 当前整体维护人为该账号的对象。
    pub owner_user_id: Option<String>,
    /// 业务组织必须属于的内部组织。
    pub business_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件，筛选不得覆盖授权。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplierReadScope {
    /// 同角色正向范围。
    pub roles: Vec<SupplierScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<SupplierScopeClause>,
}

impl SupplierScopeClause {
    /// 按与仓储条件相同的当前责任事实判断对象，不使用创建人。
    ///
    /// # 参数
    /// * `owner` - 当前或拟写入的整体维护人
    /// * `org` - 业务组织
    ///
    /// # 返回
    /// 公司、本人负责或业务组织任一命中即覆盖。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把创建人、修改人或供应商外部联系人当作维护人。
    pub fn allows(&self, owner: &str, org: &str) -> bool {
        self.company
            || self.owner_user_id.as_deref() == Some(owner)
            || self.business_org_unit_ids.iter().any(|id| id == org)
    }

    /// 没有任何可匹配责任条件时，范围确定为空。
    fn is_empty(&self) -> bool {
        !self.company && self.owner_user_id.is_none() && self.business_org_unit_ids.is_empty()
    }

    /// 将当前供应商责任转换为固定字段条件；空范围明确无结果。
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
        union(conditions)
    }
}

impl SupplierReadScope {
    /// 判断完整供应商集合是否已获授权，个人上限仍须允许公司范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅在角色公司授权且未被个人上限收窄时返回 true。
    ///
    /// # 错误
    /// 无。
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
    pub fn is_empty(&self) -> bool {
        self.roles.iter().all(SupplierScopeClause::is_empty)
            || self.user_limit.as_ref().is_some_and(SupplierScopeClause::is_empty)
    }

    /// 生成角色并集与个人上限交集。
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
    /// 历史参与不允许；能力负责人不进入授权文档。
    pub fn document(&self) -> Document {
        if self.is_company() {
            return doc! {};
        }
        let grants = self.roles.iter().map(SupplierScopeClause::document).collect::<Vec<_>>();
        let roles = union(grants);
        if let Some(limit) = &self.user_limit {
            return doc! { "$and": [roles, limit.document()] };
        }
        roles
    }

    /// 按与 [`SupplierReadScope::document`] 相同的责任事实判断单对象。
    ///
    /// # 参数
    /// * `owner` - 对象整体维护人
    /// * `org` - 对象业务组织
    ///
    /// # 返回
    /// 角色并集命中且个人上限允许时为 true。
    ///
    /// # 错误
    /// 无。
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

/// 跨页校验使用的供应商身份和版本，不包含展示字段。
#[derive(Debug, Deserialize, Hash)]
pub struct SupplierVersion {
    /// 供应商稳定主键。
    pub id: String,
    /// 供应商乐观锁版本。
    pub version: u64,
}

impl SupplierAccountRepository<'_> {
    /// 按明确授权条件读取单个供应商。
    ///
    /// # 参数
    /// * `id` - 供应商稳定主键
    /// * `scope` - 已证明的授权条件
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 `None`。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// ID 条件不能替换范围交集；空授权不得返回文档。
    pub async fn find_authorized(
        &self,
        id: &str,
        scope: &SupplierReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor).await
    }

    /// 装载查询的有界身份与版本集合，用于跨页及导出一致性校验。
    ///
    /// # 参数
    /// * `filter` - 已与授权条件求交的列表筛选
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    pub async fn query_versions(
        &self,
        filter: &SupplierAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierVersion>> {
        mongo_ops::find_many(
            &self.collection().clone_with_type::<SupplierVersion>(),
            filter.to_doc(),
            FindOptions::builder()
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
    fn responsibility_uses_maintainer_and_org() {
        let scope = SupplierReadScope {
            roles: vec![SupplierScopeClause {
                owner_user_id: Some("buyer-a".into()),
                business_org_unit_ids: vec!["org-a".into()],
                ..Default::default()
            }],
            user_limit: None,
        };
        assert!(!scope.is_empty());
        assert!(scope.allows_object("buyer-a", "org-b"));
        assert!(scope.allows_object("buyer-b", "org-a"));
        assert!(!scope.allows_object("buyer-b", "org-b"));
        assert!(!scope.allows_object("created-by", "org-b"));
    }

    #[test]
    fn missing_scope_stays_restricted() {
        assert_eq!(SupplierReadScope::default().document(), doc! { "$expr": false });
        assert!(SupplierReadScope::default().is_empty());
        let company = SupplierReadScope {
            roles: vec![SupplierScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        };
        assert!(company.is_company());
        assert_eq!(company.document(), doc! {});
    }

    #[test]
    fn owner_and_capability_filters_are_not_interchangeable() {
        let maintainer = SupplierScopeClause { owner_user_id: Some("buyer-a".into()), ..Default::default() };
        assert!(maintainer.allows("buyer-a", "org-a"));
        assert!(!maintainer.allows("cap-owner", "org-a"));
    }
}
