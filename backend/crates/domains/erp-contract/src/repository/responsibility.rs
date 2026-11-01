//! 合同独立销售责任的查询条件；共同客户不构成合同授权。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

use super::extensions::ContractExt;
use crate::entity::contract::Contract;

/// 单个已解析条款中的合同负责销售与明确共享人员。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponsibilityClause {
    /// 明确的公司范围。
    pub company: bool,
    /// 本人及已展开组织成员的账号。
    pub owners: Vec<String>,
    /// 仅用于读取的明确共享账号。
    pub shared_user: Option<String>,
}

impl ResponsibilityClause {
    fn document(&self) -> Document {
        if self.company {
            return doc! {};
        }
        let mut choices = vec![doc! { "sales_owner_user_id": { "$in": &self.owners, "$ne": "" } }];
        if let Some(user) = &self.shared_user {
            choices.push(doc! { "shared_sales_user_ids": user });
        }
        doc! { "$or": choices }
    }

    fn allows(&self, contract: &Contract) -> bool {
        self.company
            || (!contract.sales_owner_user_id.is_empty()
                && self.owners.contains(&contract.sales_owner_user_id))
            || self.shared_user.as_ref().is_some_and(|user| contract.shared_sales_user_ids.contains(user))
    }
}

/// 角色范围并集与个人上限交集，保持历史参与的单据边界。
#[derive(Debug, Clone, Default)]
pub struct ResponsibilityScope {
    /// 角色正向范围。
    pub roles: Vec<ResponsibilityClause>,
    /// 个人范围上限。
    pub limit: Option<ResponsibilityClause>,
    /// 当前动作允许的历史参与合同。
    pub history: Vec<String>,
}

impl ResponsibilityScope {
    /// 编译负责销售和明确共享的合同查询。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 缺少正向范围时返回恒假条件。
    /// # 错误
    /// 无。
    pub fn document(&self) -> Document {
        let mut roles = self.roles.iter().map(ResponsibilityClause::document).collect::<Vec<_>>();
        if !roles.is_empty() && !self.history.is_empty() {
            roles.push(doc! { "id": { "$in": &self.history } });
        }
        let positive = if roles.is_empty() {
            doc! { "$expr": false }
        } else {
            doc! { "$or": roles }
        };
        match &self.limit {
            Some(limit) => doc! { "$and": [positive, limit.document()] },
            None => positive,
        }
    }

    /// 判定已装载合同的独立销售责任。
    ///
    /// # 参数
    /// * `contract` - 合同根记录
    /// # 返回
    /// 正向范围及个人上限同时允许时返回真。
    /// # 错误
    /// 无。
    pub fn allows(&self, contract: &Contract) -> bool {
        (!self.roles.is_empty()
            && (self.roles.iter().any(|clause| clause.allows(contract))
                || self.history.contains(&contract.base.id)))
            && self.limit.as_ref().is_none_or(|limit| limit.allows(contract))
    }

    /// 解析为有界合同身份，明确公司范围保留为空条件。
    ///
    /// # 参数
    /// * `db` - 合同领域数据库
    /// * `executor` - 原授权读取执行器
    /// # 返回
    /// 公司范围返回 None，否则最多返回 10001 个合同身份供调用方拒绝超限。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    pub async fn contract_ids(
        &self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        if self.roles.iter().any(|role| role.company) && self.limit.as_ref().is_none_or(|limit| limit.company)
        {
            return Ok(None);
        }
        #[derive(Deserialize)]
        struct Id {
            id: String,
        }
        let collection = db.collection::<Id>(<Database as ContractExt>::CONTRACTS);
        let rows = mongo_ops::find_many(
            &collection,
            doc! { "$and": [self.document(), { "deleted_at": 0_i64 }] },
            FindOptions::builder().projection(doc! { "id": 1 }).sort(doc! { "id": 1 }).limit(10_001).build(),
            executor,
        )
        .await?;
        Ok(Some(rows.into_iter().map(|row| row.id).collect()))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{CustomerAccountId, PartyId};

    use super::*;
    use crate::entity::contract::{ContractData, ContractId};

    fn contract(owner: &str) -> Contract {
        Contract::new(
            ContractId::new(owner),
            ContractData {
                contract_no: owner.into(),
                customer_id: CustomerAccountId::new("same-customer"),
                settlement_party_id: PartyId::new("company"),
            },
            owner,
        )
        .unwrap()
    }

    #[test]
    fn shared_customer_does_not_share_contract_and_personal_limit_caps_company() {
        let own = contract("sales-a");
        let other = contract("sales-b");
        let mine = ResponsibilityClause { owners: vec!["sales-a".into()], ..Default::default() };
        let mut scope = ResponsibilityScope { roles: vec![mine.clone()], ..Default::default() };
        assert!(scope.allows(&own));
        assert!(!scope.allows(&other));
        scope.roles = vec![ResponsibilityClause { company: true, ..Default::default() }];
        scope.limit = Some(mine);
        assert!(scope.allows(&own));
        assert!(!scope.allows(&other));
        assert!(!ResponsibilityScope::default().allows(&own));
    }

    #[test]
    fn explicit_contract_share_and_history_do_not_change_sales_owner() {
        let mut other = contract("sales-b");
        let scope = ResponsibilityScope {
            roles: vec![ResponsibilityClause { shared_user: Some("sales-a".into()), ..Default::default() }],
            ..Default::default()
        };
        assert!(!scope.allows(&other));
        other.shared_sales_user_ids.push("sales-a".into());
        assert!(scope.allows(&other));
        assert_eq!(other.sales_owner_user_id, "sales-b");
        let history = ResponsibilityScope {
            roles: vec![ResponsibilityClause::default()],
            history: vec![other.base.id.clone()],
            ..Default::default()
        };
        assert!(history.allows(&other));
        assert!(
            !ResponsibilityScope { history: vec![other.base.id.clone()], ..Default::default() }
                .allows(&other)
        );
    }
    #[test]
    fn legacy_owner_is_not_inferred_from_shared_customer_and_sharing_keeps_owner() {
        let mut legacy = contract("sales-a");
        legacy.sales_owner_user_id.clear();
        let mine = ResponsibilityScope {
            roles: vec![ResponsibilityClause { owners: vec!["sales-a".into()], ..Default::default() }],
            ..Default::default()
        };
        assert!(!mine.allows(&legacy));
        let company = ResponsibilityScope {
            roles: vec![ResponsibilityClause { company: true, ..Default::default() }],
            ..Default::default()
        };
        assert!(company.allows(&legacy));
        let mut owned = contract("sales-a");
        owned.set_shared_sales(vec!["sales-b".into(), " sales-b ".into(), "sales-a".into()]).unwrap();
        assert_eq!(owned.sales_owner_user_id, "sales-a");
        assert_eq!(owned.shared_sales_user_ids, vec!["sales-b"]);
        assert!(owned.set_shared_sales(vec![" ".into()]).is_err());
        assert_eq!(owned.shared_sales_user_ids, vec!["sales-b"]);
    }
}
