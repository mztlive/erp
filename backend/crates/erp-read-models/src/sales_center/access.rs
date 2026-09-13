//! 报表按销售单当前访问责任授权，历史快照仅用于贡献分组。

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_customer::{AssignmentRole, CustomerExt};
use erp_identity::access_control::ScopeClause;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::Permission;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::sales_order::scope::{SalesReadScope, SalesScopeClause};
use erp_sales::repository::SalesOrderExt;
use erp_workflow::DocumentRegistryExt;
use persistence_core::Executor;
use persistence_core::Transactional;
use std::hash::{Hash, Hasher};

use crate::{Error, Result};
use erp_identity::SharedRbacService;
use mongodb::Database;

/// 销售对象读取范围；供报表、详情和关联分配查询复用。
#[derive(Clone)]
pub struct SalesAccess {
    db: Database,
    rbac: SharedRbacService,
}

impl SalesAccess {
    /// 绑定应用的身份与销售事实来源。
    ///
    /// # 返回
    /// 返回无授权缓存的读取服务，构造不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 在独立事务中重验详情权限和当前责任，返回业务版本绑定的范围版本。
    ///
    /// # 返回
    /// 返回已授权订单及范围版本；历史业绩不提供访问资格。
    ///
    /// # 错误
    /// 账号、动作或对象资格失效时拒绝；不可见订单与不存在订单均返回 NotFound。
    pub async fn detail(&self, actor: &AuditActor, id: &str) -> Result<(SalesOrder, String)> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (context, scope) = this.resolve(&actor, "detail", &[], executor).await?;
                    let order = this
                        .db
                        .sales_orders()
                        .find_authorized(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
                    let version = format!(
                        "{}:{}:{}",
                        context.scope_version, order.base.id, order.base.version
                    );
                    Ok((order, version))
                })
            })
            .await
    }
    /// 在业务读取事务内证明同角色收入及成本权限，再映射当前责任条件。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        self.resolve_resource(actor, "sales_order", action, permissions, executor)
            .await
    }

    /// 按资源动作证明范围，并使用关联销售责任解释内部组织及个人范围。
    ///
    /// # 返回
    /// 返回身份上下文和销售责任条件，成本等消费者必须额外与销售访问范围求交。
    ///
    /// # 错误
    /// 未注册资源、失效动作或查询超限均拒绝。
    pub async fn resolve_resource(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        let mut access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, resource, action, permissions, executor)
            .await?;
        let customers = self
            .collaborating_customers(actor.id(), business_date(access.as_of)?, executor)
            .await?;
        let history = if allows_history(resource, action) {
            self.participant_orders(actor.id(), executor).await?
        } else {
            Vec::new()
        };
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
            return Err(Error::ValidationError("历史参与范围超过查询上限".into()));
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
                "协作范围超过查询上限，请收窄授权范围".into(),
            ));
        }
        Ok(customers)
    }
}

/// 参与关系仅补充销售单明确登记的读取动作，不能为创建或状态迁移提供资格。
fn allows_history(resource: &str, action: &str) -> bool {
    resource == "sales_order" && matches!(action, "list" | "detail")
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
        required_scopes: vec![],
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
    fn historical_participation_never_grants_commands_or_another_resource() {
        for action in ["list", "detail"] {
            assert!(allows_history("sales_order", action));
            assert!(!allows_history("cost_entry", action));
        }
        for action in [
            "create",
            "update",
            "submit",
            "cancel_approval",
            "delete",
            "transfer",
            "*",
        ] {
            assert!(!allows_history("sales_order", action));
        }
    }

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
