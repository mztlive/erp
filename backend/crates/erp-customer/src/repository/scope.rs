//! 客户授权条件；由应用层完成 DataScope 解析后再交给仓储。

use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use persistence_core::Executor;
use persistence_core::{mongo_ops, QueryFilter, Result};
use serde::Deserialize;

use super::customer::CustomerAccountFilter;
use super::owned::{CustomerAccountRepository, CustomerAssignmentRepository};
use crate::entity::customer::{CustomerAccount, CustomerAssignment};

/// 同一角色已证明的客户责任条件；主责、协作与负责人组织分别解释。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomerScopeClause {
    /// 公司范围覆盖该资源动作的全部客户。
    pub company: bool,
    /// 当前主负责人为该账号的客户。
    pub owner_user_id: Option<String>,
    /// 当前协作者可见的客户 ID。
    pub collaborative_customer_ids: Vec<String>,
    /// 当前主负责人必须属于的内部组织。
    pub owner_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件；历史参与只补充读取。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerReadScope {
    /// 同角色正向范围。
    pub roles: Vec<CustomerScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<CustomerScopeClause>,
    /// 已证明的历史参与客户；不得用于修改。
    pub historical_customer_ids: Vec<String>,
    /// 当前主责客户，供目录「我的客户」收窄。
    pub owned_customer_ids: Vec<String>,
    /// 当前协作客户，供目录「协作客户」收窄。
    pub collaborative_customer_ids: Vec<String>,
    /// 已转换成仓储条件的授权客户集合；`None` 表示公司范围。
    pub authorized_customer_ids: Option<Vec<String>>,
}

impl Default for CustomerReadScope {
    /// 缺省授权为空集，禁止未解析范围退化为公司范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回不含角色、上限和历史参与的空授权。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// `authorized_customer_ids = Some([])` 才是空集；`None` 表示公司范围。
    fn default() -> Self {
        Self {
            roles: Vec::new(),
            user_limit: None,
            historical_customer_ids: Vec::new(),
            owned_customer_ids: Vec::new(),
            collaborative_customer_ids: Vec::new(),
            authorized_customer_ids: Some(Vec::new()),
        }
    }
}

/// 跨页校验使用的客户身份和版本，不包含展示字段。
#[derive(Debug, Deserialize, Hash)]
pub struct CustomerVersion {
    /// 客户稳定主键。
    pub id: String,
    /// 客户乐观锁版本。
    pub version: u64,
}

impl CustomerScopeClause {
    /// 判断尚未持久化或已装载的客户责任是否被该条款覆盖。
    ///
    /// # 参数
    /// * `owner` - 当前或拟写入的主负责人
    /// * `owner_org` - 主负责人当前主属组织
    /// * `collaborating` - 当前账号是否为协作销售
    ///
    /// # 返回
    /// 公司、主责、协作或负责人组织任一命中即覆盖；空条款不覆盖。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把创建人当作主责；组织条件只解释当前主负责人所属组织。
    pub fn allows(&self, owner: &str, owner_org: Option<&str>, collaborating: bool) -> bool {
        self.company
            || self.owner_user_id.as_deref() == Some(owner)
            || (self.collaborative() && collaborating)
            || owner_org.is_some_and(|org| self.owner_org_unit_ids.iter().any(|id| id == org))
    }

    /// 判断该条款是否包含协作范围。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 存在协作客户集合时返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 空集合不得解释为全部协作客户。
    pub fn collaborative(&self) -> bool {
        !self.collaborative_customer_ids.is_empty()
    }

    /// 判断该条款是否确定不产生可见对象。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 非公司且没有任何主责、协作或组织条件时为空。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 空条款必须保持空集，不得补公司范围。
    pub fn is_empty(&self) -> bool {
        !self.company
            && self.owner_user_id.is_none()
            && self.collaborative_customer_ids.is_empty()
            && self.owner_org_unit_ids.is_empty()
    }
}

impl CustomerReadScope {
    /// 校验创建客户时的拟写入责任；历史参与不授予创建权。
    ///
    /// # 参数
    /// * `owner` - 拟写入的主负责人，必须是当前操作人
    /// * `owner_org` - 操作人当前主属组织
    ///
    /// # 返回
    /// 角色并集覆盖且个人上限允许时返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 创建固定以操作人为主责，协作身份与历史参与不得单独放行建档。
    pub fn allows_creation(&self, owner: &str, owner_org: Option<&str>) -> bool {
        self.roles
            .iter()
            .any(|clause| clause.allows(owner, owner_org, false))
            && self
                .user_limit
                .as_ref()
                .is_none_or(|clause| clause.allows(owner, owner_org, false))
    }

    /// 判断指定客户是否落在已证明的授权集合内。
    ///
    /// # 参数
    /// * `customer_id` - 客户稳定主键
    ///
    /// # 返回
    /// 公司范围或命中授权 ID 时返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 授权集合必须由 Service 预先求值；仓储不得按登录人自行推断。
    pub fn allows_id(&self, customer_id: &str) -> bool {
        self.authorized_customer_ids
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|id| id == customer_id))
    }

    /// 判断授权是否覆盖全部未删除客户。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅在角色公司授权且未被个人上限收窄时为 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 公司范围仍受个人上限约束，不得用其补齐缺范围角色。
    pub fn is_company(&self) -> bool {
        self.authorized_customer_ids.is_none()
    }

    /// 判断授权规则是否确定不产生可见对象。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 空授权返回 true；非空规则仍可能被业务筛选收成空页。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 缺范围必须保持空集，不得退化为全量查询。
    pub fn is_empty(&self) -> bool {
        self.authorized_customer_ids.as_ref().is_some_and(Vec::is_empty)
    }

    /// 生成客户角色集合上的授权条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 公司范围为恒真；明确 ID 使用 `$in`；空集使用恒假表达式。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// MongoDB 不接受空 `$in` 作为全量；空授权必须拒绝匹配。
    pub fn document(&self) -> Document {
        match &self.authorized_customer_ids {
            None => doc! {},
            Some(ids) if ids.is_empty() => doc! { "$expr": false },
            Some(ids) => doc! { "id": { "$in": ids } },
        }
    }
}

impl CustomerAccountRepository<'_> {
    /// 按明确授权条件读取单个客户。
    ///
    /// # 参数
    /// * `id` - 客户稳定主键
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
        scope: &CustomerReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerAccount>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor)
            .await
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
    ///
    /// # 关键业务约束
    /// 不得截断版本集合后继续拼接跨页结果。
    pub async fn query_versions(
        &self,
        filter: &CustomerAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerVersion>> {
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CustomerVersion>(),
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

impl CustomerAssignmentRepository<'_> {
    /// 读取某账号全部未删除归属，供读取动作补充历史参与。
    ///
    /// # 参数
    /// * `user_id` - 销售人员
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回该人员全部归属行，含已结束窗口。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 历史参与只用于读取；调用方不得把本结果当作修改资格。
    pub async fn list_for_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        self.find_many(doc! { "user_id": user_id }, executor).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_role_scope_rejects_create() {
        let empty = CustomerReadScope::default();
        assert!(!empty.allows_creation("sales-a", Some("org-a")));
        let self_owned = CustomerReadScope {
            roles: vec![CustomerScopeClause {
                owner_user_id: Some("sales-a".into()),
                ..Default::default()
            }],
            authorized_customer_ids: Some(Vec::new()),
            owned_customer_ids: vec![],
            collaborative_customer_ids: vec![],
            historical_customer_ids: vec![],
            user_limit: None,
        };
        assert!(self_owned.is_empty());
        assert!(self_owned.allows_creation("sales-a", Some("org-a")));
    }

    #[test]
    fn creation_requires_role_and_personal_limit_without_history() {
        let mut scope = CustomerReadScope {
            roles: vec![CustomerScopeClause {
                company: true,
                ..Default::default()
            }],
            authorized_customer_ids: None,
            owned_customer_ids: vec![],
            collaborative_customer_ids: vec![],
            historical_customer_ids: vec![],
            user_limit: None,
        };
        assert!(scope.allows_creation("sales-a", Some("org-a")));
        scope.user_limit = Some(CustomerScopeClause {
            owner_org_unit_ids: vec!["org-b".into()],
            ..Default::default()
        });
        assert!(!scope.allows_creation("sales-a", Some("org-a")));
        assert!(scope.allows_creation("sales-a", Some("org-b")));
        scope.historical_customer_ids.push("old-customer".into());
        scope.roles.clear();
        scope.authorized_customer_ids = Some(vec!["old-customer".into()]);
        assert!(!scope.allows_creation("sales-a", Some("org-b")));
    }

    #[test]
    fn owner_and_collaborator_are_not_interchangeable() {
        let owner = CustomerScopeClause {
            owner_user_id: Some("sales-a".into()),
            ..Default::default()
        };
        let collab = CustomerScopeClause {
            collaborative_customer_ids: vec!["customer-b".into()],
            ..Default::default()
        };
        assert!(owner.allows("sales-a", Some("org-a"), false));
        assert!(!owner.allows("sales-b", Some("org-a"), true));
        assert!(collab.allows("sales-b", Some("org-a"), true));
        assert!(!collab.allows("sales-b", Some("org-a"), false));
    }

    #[test]
    fn missing_scope_stays_restricted_and_company_is_explicit() {
        assert_eq!(CustomerReadScope::default().document(), doc! { "$expr": false });
        assert!(CustomerReadScope::default().is_empty());
        assert!(!CustomerReadScope::default().is_company());
        let company = CustomerReadScope {
            authorized_customer_ids: None,
            roles: vec![CustomerScopeClause {
                company: true,
                ..Default::default()
            }],
            user_limit: None,
            historical_customer_ids: vec![],
            owned_customer_ids: vec![],
            collaborative_customer_ids: vec![],
        };
        assert!(company.is_company());
        assert!(!company.is_empty());
        assert_eq!(company.document(), doc! {});
    }
}
