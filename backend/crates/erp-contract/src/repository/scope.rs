//! 合同授权条件；由应用层完成 DataScope 解析后再交给仓储。

use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use persistence_core::Executor;
use persistence_core::{mongo_ops, QueryFilter, Result};
use serde::Deserialize;

use super::contract::{ContractDomainRepository, ContractFilter};
use super::extensions::ContractExt;
use super::owned::ContractRepository;
use crate::entity::contract::Contract;

/// 同一角色已证明的合同责任条件；当前客户主责、协作与负责人组织分别解释。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractScopeClause {
    /// 公司范围覆盖该资源动作的全部合同。
    pub company: bool,
    /// 当前客户主负责人为该账号时命中。
    pub owner_user_id: Option<String>,
    /// 当前协作者可见的客户 ID。
    pub collaborative_customer_ids: Vec<String>,
    /// 当前客户主负责人必须属于的内部组织。
    pub owner_org_unit_ids: Vec<String>,
}

/// 角色并集与个人上限保留为独立条件；历史参与只补充读取。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractReadScope {
    /// 同角色正向范围。
    pub roles: Vec<ContractScopeClause>,
    /// 用户个人上限；缺省表示不附加个人限制。
    pub user_limit: Option<ContractScopeClause>,
    /// 已证明的历史参与合同；不得用于修改。
    pub historical_contract_ids: Vec<String>,
    /// 当前主责客户，供目录「已分配」收窄。
    pub owned_customer_ids: Vec<String>,
    /// 当前协作客户，供目录「已分配」收窄。
    pub collaborative_customer_ids: Vec<String>,
    /// 已转换成仓储条件的授权客户集合；`None` 表示公司范围。
    pub authorized_customer_ids: Option<Vec<String>>,
}

impl Default for ContractReadScope {
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
            historical_contract_ids: Vec::new(),
            owned_customer_ids: Vec::new(),
            collaborative_customer_ids: Vec::new(),
            authorized_customer_ids: Some(Vec::new()),
        }
    }
}

/// 跨页校验使用的合同身份和版本，不包含展示字段。
#[derive(Debug, Deserialize, Hash)]
pub struct ContractVersion {
    /// 合同稳定主键。
    pub id: String,
    /// 合同乐观锁版本。
    pub version: u64,
}

/// 历史参与合同的客户映射，供个人上限与筛选收窄。
#[derive(Debug, Deserialize)]
pub struct ContractCustomerRef {
    /// 合同稳定主键。
    pub id: String,
    /// 合同所属客户。
    pub customer_id: String,
}

impl ContractScopeClause {
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

impl ContractReadScope {
    /// 校验创建合同时拟写入客户是否被角色范围覆盖；历史参与不授予创建权。
    ///
    /// # 参数
    /// * `customer_id` - 拟归档合同的客户
    ///
    /// # 返回
    /// 授权客户集合覆盖该客户时返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 创建不得使用历史参与或签约经办作为资格。
    #[cfg(test)]
    pub fn allows_creation(&self, customer_id: &str) -> bool {
        self.allows_customer(customer_id)
    }

    /// 判断指定客户当前合同是否落在角色授权集合内。
    ///
    /// # 参数
    /// * `customer_id` - 客户稳定主键
    ///
    /// # 返回
    /// 公司范围或命中授权客户 ID 时返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不含历史参与合同；写命令必须调用本方法而非 `document`。
    pub fn allows_customer(&self, customer_id: &str) -> bool {
        self.authorized_customer_ids
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|id| id == customer_id))
    }

    /// 判断授权是否覆盖全部未删除合同。
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
    /// 空授权且无历史参与返回 true。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 缺范围必须保持空集，不得退化为全量查询。
    pub fn is_empty(&self) -> bool {
        self.authorized_customer_ids.as_ref().is_some_and(Vec::is_empty)
            && self.historical_contract_ids.is_empty()
    }

    /// 生成合同集合上的授权条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 公司范围为恒真；明确客户或历史合同使用 `$in`／`$or`；空集使用恒假表达式。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// MongoDB 不接受空 `$in` 作为全量；空授权必须拒绝匹配。
    pub fn document(&self) -> Document {
        authorization_document(
            self.authorized_customer_ids.as_deref(),
            &self.historical_contract_ids,
        )
    }
}

/// 将已证明的客户集合与历史合同编译为仓储条件。
///
/// # 参数
/// * `customer_ids` - 授权客户；`None` 表示公司范围
/// * `historical_contract_ids` - 合法历史参与合同
///
/// # 返回
/// 公司范围为恒真；空授权为恒假。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 历史参与只能并入读取条件，调用方不得把本结果当作修改资格。
pub fn authorization_document(
    customer_ids: Option<&[String]>,
    historical_contract_ids: &[String],
) -> Document {
    match customer_ids {
        None => doc! {},
        Some(customers) if customers.is_empty() && historical_contract_ids.is_empty() => {
            doc! { "$expr": false }
        }
        Some(customers) if historical_contract_ids.is_empty() => {
            doc! { "customer_id": { "$in": customers } }
        }
        Some([]) => {
            doc! { "id": { "$in": historical_contract_ids } }
        }
        Some(customers) => doc! {
            "$or": [
                { "customer_id": { "$in": customers } },
                { "id": { "$in": historical_contract_ids } },
            ]
        },
    }
}

impl ContractRepository<'_> {
    /// 按明确授权条件读取单个合同。
    ///
    /// # 参数
    /// * `id` - 合同稳定主键
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
        scope: &ContractReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<Contract>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor)
            .await
    }

    /// 装载指定合同的身份与所属客户，供历史参与收窄。
    ///
    /// # 参数
    /// * `ids` - 候选合同 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 仅返回仍然存在的合同及其客户。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 不得把其他单据类型的参与 ID 当作合同。
    pub async fn customer_refs_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractCustomerRef>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        mongo_ops::find_many(
            &self.collection().clone_with_type::<ContractCustomerRef>(),
            doc! { "id": { "$in": ids } },
            FindOptions::builder()
                .projection(doc! { "id": 1, "customer_id": 1 })
                .sort(doc! { "id": 1 })
                .limit(10_001)
                .build(),
            executor,
        )
        .await
    }
}

impl ContractDomainRepository<'_> {
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
        filter: &ContractFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ContractVersion>> {
        mongo_ops::find_many(
            &self
                .db
                .collection::<ContractVersion>(<mongodb::Database as ContractExt>::CONTRACTS),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_role_scope_rejects_create_and_missing_scope_stays_restricted() {
        let empty = ContractReadScope::default();
        assert!(!empty.allows_creation("cust-1"));
        assert!(empty.is_empty());
        assert!(!empty.is_company());
        assert_eq!(empty.document(), doc! { "$expr": false });
        let company = ContractReadScope {
            authorized_customer_ids: None,
            roles: vec![ContractScopeClause {
                company: true,
                ..Default::default()
            }],
            user_limit: None,
            historical_contract_ids: vec![],
            owned_customer_ids: vec![],
            collaborative_customer_ids: vec![],
        };
        assert!(company.is_company());
        assert!(company.allows_creation("cust-1"));
        assert_eq!(company.document(), doc! {});
    }

    #[test]
    fn history_cannot_grant_create_and_is_ored_only_for_read() {
        let scope = ContractReadScope {
            roles: vec![],
            user_limit: None,
            historical_contract_ids: vec!["ht-old".into()],
            owned_customer_ids: vec![],
            collaborative_customer_ids: vec![],
            authorized_customer_ids: Some(Vec::new()),
        };
        assert!(!scope.allows_creation("cust-1"));
        assert_eq!(scope.document(), doc! { "id": { "$in": ["ht-old"] } });
        let mixed = authorization_document(Some(&["cust-1".into()]), &["ht-old".into()]);
        assert_eq!(
            mixed,
            doc! {
                "$or": [
                    { "customer_id": { "$in": ["cust-1"] } },
                    { "id": { "$in": ["ht-old"] } },
                ]
            }
        );
    }

    #[test]
    fn personal_limit_company_is_explicit_none() {
        let limited = ContractReadScope {
            roles: vec![ContractScopeClause {
                company: true,
                ..Default::default()
            }],
            user_limit: Some(ContractScopeClause {
                owner_user_id: Some("sales-a".into()),
                ..Default::default()
            }),
            historical_contract_ids: vec![],
            owned_customer_ids: vec!["c-own".into()],
            collaborative_customer_ids: vec![],
            authorized_customer_ids: Some(vec!["c-own".into()]),
        };
        assert!(!limited.is_company());
        assert!(limited.allows_creation("c-own"));
        assert!(!limited.allows_creation("c-other"));
    }
}
