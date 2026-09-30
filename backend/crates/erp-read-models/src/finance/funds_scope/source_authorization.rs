//! 财务读取保留自身动作资格，数据边界继承实际业务来源。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::slice;

use application_core::AuditActor;
use erp_finance::ports::funds_scope::{FundsResolvedClause, FundsResolvedScope};
use erp_identity::access_control::ScopeClause;
use erp_identity::entity::policy_permission::FINANCE_LEDGER_READ;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{AccessControlExt, Permission, RoleRepositoryExt, SharedRbacService};
use erp_procurement::repository::purchase_order::scope::{PurchaseReadScope, PurchaseScopeClause};
use erp_sales::repository::sales_order::scope::{SalesReadScope, SalesScopeClause};
use mongodb::Database;
use persistence_core::Executor;

use super::{FundsAccess, FundsAuthorization};
use crate::Result;
use crate::sales_center::access::sales_scope;

impl FundsAccess {
    /// 同一事务冻结目标动作、来源范围与显式整账资格。
    ///
    /// # 参数
    /// 调用人、目标资源动作、是否包含采购/双方向来源与原事务。
    /// # 返回
    /// 返回真实来源授权及整账职责；不读取退休资金范围。
    /// # 错误
    /// 账号、静态动作、来源注册、策略快照或持久化错误时拒绝。
    pub(super) async fn resolve_sources(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        purchase: bool,
        dual: bool,
        executor: &mut dyn Executor,
    ) -> Result<(FundsResolvedScope, FundsAuthorization)> {
        let resolver = DataScopeService::new(self.db.clone(), self.rbac.clone());
        let source = if purchase { "purchase_order" } else { "sales_order" };
        let mut context =
            resolver.resolve_source_scope(actor, resource, action, source, "list", executor).await?;
        let mut fingerprint = DefaultHasher::new();
        context.scope_version.hash(&mut fingerprint);
        let primary = sales_scope(&context, actor.id(), &[], Vec::new());
        let purchase_scope = purchase.then(|| purchase_condition(&primary));
        let sales = if dual {
            let access = resolver
                .resolve_source_scope(actor, resource, action, "sales_order", "list", executor)
                .await?;
            access.scope_version.hash(&mut fingerprint);
            sales_scope(&access, actor.id(), &[], Vec::new())
        } else if purchase {
            SalesReadScope::default()
        } else {
            primary
        };
        let settlement = if purchase {
            let access = resolver
                .resolve_source_scope(
                    actor,
                    resource,
                    action,
                    "supplier_settlement_statement",
                    "list",
                    executor,
                )
                .await?;
            access.scope_version.hash(&mut fingerprint);
            Some(access)
        } else {
            None
        };
        let ledger_read = ledger_readable(&self.db, &self.rbac, actor, executor).await?;
        ledger_read.hash(&mut fingerprint);
        context.scope_version = format!("{:x}", fingerprint.finish());
        let access = resolved_funds(&context);
        let mut authorization =
            FundsAuthorization { sales, ledger_read, settlement, purchase_scope, context, no_scope: false };
        authorization.no_scope = authorization.empty();
        Ok((access, authorization))
    }
}

/// 显式整账职责只由当前启用角色授予，并绑定调用方事务策略版本。
///
/// # 参数
/// 数据库、RBAC 服务、调用人及原事务执行器。
/// # 返回
/// 当前启用角色授予整账职责时返回 true。
/// # 错误
/// 策略快照失效或持久化失败时返回错误。
pub(crate) async fn ledger_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let permission = Permission::parse(FINANCE_LEDGER_READ)?;
    let snapshot =
        rbac.role_permission_snapshot(actor.kind(), actor.id(), slice::from_ref(&permission)).await?;
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    let roles = db.roles().enabled_roles(&snapshot.granting_role_ids(&permission), executor).await?;
    Ok(!roles.is_empty())
}

/// 把已经解析的来源边界投影为现有资金读取输入；不读取退休的资金授权记录。
fn resolved_funds(context: &AuthorizedDataScope) -> FundsResolvedScope {
    let clause = |scope: &ScopeClause| FundsResolvedClause {
        company: scope.company,
        self_owned: scope.self_owned,
        collaborative: scope.collaborative,
        org_unit_ids: scope.org_unit_ids.iter().cloned().collect(),
    };
    FundsResolvedScope {
        user_id: context.user_id.clone(),
        resource: context.resource.clone(),
        action: context.action.clone(),
        role_clauses: context.scope.role_clauses.iter().map(clause).collect(),
        user_limit: context.scope.user_limit.as_ref().map(clause),
        policy_version: context.policy_version,
        organization_version: context.organizations.version,
        scope_version: context.scope_version.clone(),
        as_of: context.as_of,
    }
}

/// 采购来源只接受当前采购责任及组织，不复制销售历史或协作资格。
fn purchase_condition(source: &SalesReadScope) -> PurchaseReadScope {
    let clause = |scope: &SalesScopeClause| PurchaseScopeClause {
        company: scope.company,
        owner_user_id: scope.owner_user_id.clone(),
        business_org_unit_ids: scope.business_org_unit_ids.clone(),
    };
    PurchaseReadScope {
        required_scopes: Vec::new(),
        roles: source.roles.iter().map(clause).collect(),
        user_limit: source.user_limit.as_ref().map(clause),
        historical_order_ids: Vec::new(),
    }
}
