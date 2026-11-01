//! 合同按独立销售责任解析范围，与客户资料共享范围分离。

use std::collections::BTreeSet;

use persistence_core::Executor;

use super::{ContractAccess, allows_history, ensure_limit};
use crate::entity::contract::Contract;
use crate::error::Result;
use crate::ports::{ContractResolvedClause, ContractResolvedScope, ContractScopeObject};
use crate::repository::responsibility::{ResponsibilityClause, ResponsibilityScope};

impl ContractAccess {
    /// 将同角色范围编译成合同负责销售或明确授权的身份集合。
    pub(super) async fn responsible_ids(
        &self,
        access: &ContractResolvedScope,
        history: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        let mut roles = Vec::new();
        for clause in &access.role_clauses {
            if clause.has_scope_rules() {
                roles.push(self.responsible_clause(access, clause, executor).await?);
            }
        }
        let limit = match &access.user_limit {
            Some(clause) => Some(self.responsible_clause(access, clause, executor).await?),
            None => None,
        };
        let scope = ResponsibilityScope { roles, limit, history: history.to_vec() };
        let ids = scope.contract_ids(&self.db, executor).await?;
        if let Some(ids) = &ids {
            ensure_limit(ids.len(), "合同责任范围超过查询上限，请收窄组织范围")?;
        }
        Ok(ids)
    }

    async fn responsible_clause(
        &self,
        access: &ContractResolvedScope,
        clause: &ContractResolvedClause,
        executor: &mut dyn Executor,
    ) -> Result<ResponsibilityClause> {
        let mut owners = if clause.self_owned { vec![access.user_id.clone()] } else { Vec::new() };
        if !clause.org_unit_ids.is_empty() {
            let orgs = clause.org_unit_ids.iter().cloned().collect::<BTreeSet<_>>();
            owners.extend(self.scope.org_member_ids(&orgs, access.as_of, executor).await?);
        }
        owners.sort();
        owners.dedup();
        ensure_limit(owners.len(), "合同负责销售范围超过查询上限")?;
        Ok(ResponsibilityClause {
            company: clause.company,
            owners,
            shared_user: (allows_history(&access.action) && (clause.self_owned || clause.collaborative))
                .then(|| access.user_id.clone()),
        })
    }

    /// 单对象判定只使用合同责任；明确共享不授予修改资格。
    pub(super) async fn contract_object(
        &self,
        contract: &Contract,
        access: &ContractResolvedScope,
        executor: &mut dyn Executor,
    ) -> Result<ContractScopeObject> {
        let shared =
            allows_history(&access.action) && contract.shared_sales_user_ids.contains(&access.user_id);
        let org_unit_id = if contract.sales_owner_user_id.is_empty() {
            None
        } else {
            self.scope.own_org(&contract.sales_owner_user_id, access.as_of, executor).await?
        };
        Ok(ContractScopeObject {
            owned: contract.sales_owner_user_id == access.user_id || shared,
            collaborating: shared,
            historical_read_participant: false,
            org_unit_id,
        })
    }
}
