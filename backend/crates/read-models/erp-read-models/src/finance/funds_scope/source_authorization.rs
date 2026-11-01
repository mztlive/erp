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
    /// * `actor` - 已认证操作人。
    /// * `resource` - 目标资金资源。
    /// * `action` - 目标动作。
    /// * `purchase` - 为 true 时解析采购与结算来源。
    /// * `dual` - 为 true 时额外解析销售来源。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 返回真实来源授权及整账职责；不读取退休资金范围。
    ///
    /// # 错误
    /// 权限解析、来源范围解析或整账资格读取失败时返回对应错误。
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
        let ledger_permission = Permission::parse(FINANCE_LEDGER_READ)?;
        let permissions = [Permission::parse(format!("{resource}:{action}"))?, ledger_permission.clone()];
        let mut batch = resolver.batch(actor, &permissions, executor);
        let source = if purchase { "purchase_order" } else { "sales_order" };
        let context = batch.resolve_source_scope(resource, action, source, "list").await?;
        let sales = if dual {
            Some(batch.resolve_source_scope(resource, action, "sales_order", "list").await?)
        } else {
            None
        };
        let settlement = if purchase {
            Some(batch.resolve_source_scope(resource, action, "supplier_settlement_statement", "list").await?)
        } else {
            None
        };
        let ledger_read = batch.has_permission(&ledger_permission).await?;
        Ok(source_authorization(context, sales, settlement, actor.id(), purchase, ledger_read))
    }
}

/// 每个真实来源的独立指纹按旧顺序绑定，整账资格不能替代来源范围。
fn source_authorization(
    mut context: AuthorizedDataScope,
    additional_sales: Option<AuthorizedDataScope>,
    settlement: Option<AuthorizedDataScope>,
    actor_id: &str,
    purchase: bool,
    ledger_read: bool,
) -> (FundsResolvedScope, FundsAuthorization) {
    let mut fingerprint = DefaultHasher::new();
    context.scope_version.hash(&mut fingerprint);
    let primary = sales_scope(&context, actor_id, &[], Vec::new());
    let purchase_scope = purchase.then(|| purchase_condition(&primary));
    let sales = if let Some(access) = additional_sales {
        access.scope_version.hash(&mut fingerprint);
        sales_scope(&access, actor_id, &[], Vec::new())
    } else if purchase {
        SalesReadScope::default()
    } else {
        primary
    };
    if let Some(access) = &settlement {
        access.scope_version.hash(&mut fingerprint);
    }
    ledger_read.hash(&mut fingerprint);
    context.scope_version = format!("{:x}", fingerprint.finish());
    let access = resolved_funds(&context);
    let mut authorization =
        FundsAuthorization { sales, ledger_read, settlement, purchase_scope, context, no_scope: false };
    authorization.no_scope = authorization.empty();
    (access, authorization)
}

/// 显式整账职责只由当前启用角色授予，并绑定调用方事务策略版本。
///
/// # 参数
/// * `db` - 角色集合所在数据库。
/// * `rbac` - 当前 RBAC 快照服务。
/// * `actor` - 已认证操作人。
/// * `executor` - 调用方事务。
///
/// # 返回
/// 当前启用角色授予整账职责时返回 true。
///
/// # 错误
/// 策略快照失效或角色读取失败时返回对应错误。
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

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_identity::access_control::ResolvedScope;

    use super::*;

    /// 构造独立来源上下文，以范围指纹代表该来源已完成的当前资格取证。
    fn context(version: &str, clause: Option<ScopeClause>) -> AuthorizedDataScope {
        AuthorizedDataScope {
            user_id: "actor".into(),
            resource: "invoice".into(),
            action: "list".into(),
            scope: ResolvedScope { role_clauses: clause.into_iter().collect(), user_limit: None },
            role_scopes: Default::default(),
            organizations: Default::default(),
            policy_version: 1,
            scope_version: version.into(),
            as_of: Instant::from_unix_secs(1),
        }
    }

    /// 发票双来源分别保留销售与采购实际范围，各来源撤权与整账变化都使版本失效。
    #[test]
    fn source_versions_bind_each_independent_scope_and_ledger() {
        let build = |purchase_version: &str, sales_version: &str, settlement_version: &str, ledger| {
            source_authorization(
                context(purchase_version, Some(ScopeClause { self_owned: true, ..Default::default() })),
                Some(context(sales_version, Some(ScopeClause { company: true, ..Default::default() }))),
                Some(context(settlement_version, None)),
                "actor",
                true,
                ledger,
            )
        };
        let (access, authorization) = build("purchase", "sales", "settlement", false);
        assert_eq!(access.resource, "invoice");
        assert_eq!(access.scope_version, authorization.context.scope_version);
        assert!(authorization.sales.is_company());
        let purchase = authorization.purchase_scope.unwrap();
        assert_eq!(purchase.roles[0].owner_user_id.as_deref(), Some("actor"));
        assert!(!purchase.roles[0].company);
        let mut legacy = DefaultHasher::new();
        for version in ["purchase", "sales", "settlement"] {
            version.hash(&mut legacy);
        }
        false.hash(&mut legacy);
        assert_eq!(access.scope_version, format!("{:x}", legacy.finish()));
        for (purchase, sales, settlement, ledger) in [
            ("changed", "sales", "settlement", false),
            ("purchase", "changed", "settlement", false),
            ("purchase", "sales", "changed", false),
            ("purchase", "sales", "settlement", true),
        ] {
            assert_ne!(access.scope_version, build(purchase, sales, settlement, ledger).0.scope_version);
        }
    }

    /// 全部来源无范围时按原合同返回空；整账职责只开放无来源读取，不补来源范围。
    #[test]
    fn denied_sources_remain_empty_and_ledger_does_not_add_scope() {
        let build = |ledger| {
            source_authorization(
                context("purchase", None),
                Some(context("sales", None)),
                Some(context("settlement", None)),
                "actor",
                true,
                ledger,
            )
        };
        let (_, denied) = build(false);
        assert!(denied.no_scope);
        assert!(denied.empty());
        let (_, ledger) = build(true);
        assert!(!ledger.no_scope);
        assert!(ledger.sales.is_empty());
        assert!(ledger.purchase_scope.unwrap().is_empty());
        assert!(ledger.settlement.unwrap().scope.role_clauses.is_empty());
    }
}
