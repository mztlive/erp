//! 报表按销售单当前访问责任授权，历史快照仅用于贡献分组。

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_customer::{AssignmentRole, CustomerExt};
use erp_identity::access_control::ScopeClause;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::Permission;
use erp_sales::repository::sales_order::scope::{SalesReadScope, SalesScopeClause};
use erp_workflow::DocumentRegistryExt;
use persistence_core::Executor;
use std::hash::{Hash, Hasher};

use super::ActualProfitLossReadModel;
use crate::{Error, Result};

impl ActualProfitLossReadModel {
    /// 在业务读取事务内证明同角色收入及成本权限，再映射当前责任条件。
    pub(super) async fn authorized_scope(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        let mut access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(
                actor,
                "sales_order",
                "list",
                &[Permission::parse("cost_entry:list")?],
                executor,
            )
            .await?;
        let customers = self
            .collaborating_customers(actor.id(), business_date(access.as_of)?, executor)
            .await?;
        let history = self.participant_orders(actor.id(), executor).await?;
        // 协作关系的有效日期变化也必须使跨页凭据失效，指纹不返回客户身份集合。
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        customers.hash(&mut fingerprint);
        history.hash(&mut fingerprint);
        access.scope_version = format!("{}:{:x}", access.scope_version, fingerprint.finish());
        let scope = sales_scope(&access, actor.id(), &customers, history);
        Ok((access, scope))
    }
    /// 完整读取动作已由身份域证明，参与事实独立补充读取并继续受个人上限约束。
    async fn participant_orders(&self, user: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let ids = self
            .db
            .document_participants()
            .document_ids_by_user(user, executor)
            .await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("历史参与范围超过报表查询上限".into()));
        }
        Ok(ids)
    }
    /// 在授权事务内解析有效协作，空集合不得作为全量范围；超限整体拒绝。
    async fn collaborating_customers(
        &self,
        user: &str,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut customers = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(user, as_of, executor)
            .await?
            .into_iter()
            .filter(|a| a.assignment_role == AssignmentRole::Collaborator)
            .map(|a| a.customer_id.to_string())
            .collect::<Vec<_>>();
        customers.sort();
        customers.dedup();
        if customers.len() > 10_000 {
            return Err(Error::ValidationError(
                "协作范围超过报表查询上限，请收窄授权范围".into(),
            ));
        }
        Ok(customers)
    }
}

/// 客户自然日规则使用授权上下文的同一时点，避免查询跨上海零点时混用两天的资格。
fn business_date(at: Instant) -> Result<BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
}

/// 映射同角色正向范围和独立个人上限，保持两者的交集关系。
fn sales_scope(
    access: &AuthorizedDataScope,
    user: &str,
    customers: &[String],
    history: Vec<String>,
) -> SalesReadScope {
    SalesReadScope {
        historical_order_ids: history,
        roles: access
            .scope
            .role_clauses
            .iter()
            .map(|c| clause(c, user, customers))
            .collect(),
        user_limit: access
            .scope
            .user_limit
            .as_ref()
            .map(|c| clause(c, user, customers)),
    }
}

/// 仅接受身份域已解析的内部组织范围，不读取历史业绩或创建人作为授权。
fn clause(scope: &ScopeClause, actor: &str, customers: &[String]) -> SalesScopeClause {
    SalesScopeClause {
        company: scope.company,
        owner_user_id: scope.self_owned.then(|| actor.into()),
        business_org_unit_ids: scope.org_unit_ids.iter().cloned().collect(),
        collaborative_customer_ids: if scope.collaborative {
            customers.to_vec()
        } else {
            Vec::new()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_qualification_uses_the_authorization_instant_at_shanghai_midnight() {
        let before = chrono::DateTime::parse_from_rfc3339("2026-09-13T15:59:59Z").unwrap();
        let after = chrono::DateTime::parse_from_rfc3339("2026-09-13T16:00:00Z").unwrap();
        assert_eq!(
            business_date(Instant::from_unix_secs(before.timestamp()))
                .unwrap()
                .to_string(),
            "2026-09-13"
        );
        assert_eq!(
            business_date(Instant::from_unix_secs(after.timestamp()))
                .unwrap()
                .to_string(),
            "2026-09-14"
        );
    }
}
