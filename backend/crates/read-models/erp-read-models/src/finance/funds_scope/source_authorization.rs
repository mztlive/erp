//! 财务入口统一证明操作权限及真实来源数据资格。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::policy_permission::FINANCE_LEDGER_READ;
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission, SharedRbacService};
use erp_procurement::authorization::PurchaseMapping;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use mongodb::Database;
use persistence_core::Executor;

use super::authorization::FundsReadContext;
use super::repository::settlement_mapping::SettlementMapping;
use super::repository::source_access::{SourceQuery, source_query};
use super::repository::whole_access::WholeAccess;
use super::{FundsAccess, FundsAuthorization};
use crate::Result;
use crate::sales_center::access::SalesAccess;

impl FundsAccess {
    /// 在同一执行器取得资金入口及各实际来源的完整查询凭证。
    /// # 参数
    /// actor 为账号，resource/action 为固定入口，purchase/dual 指定实际业务来源。
    /// # 返回
    /// 本阶段元信息和可执行来源凭证。
    /// # 错误
    /// 身份、入口、来源事实或映射失效时拒绝。
    pub(super) async fn resolve_sources(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        purchase: bool,
        dual: bool,
        executor: &mut dyn Executor,
    ) -> Result<(FundsReadContext, FundsAuthorization)> {
        let entry = Permission::parse(format!("{resource}:{action}"))?;
        let mut versions = Vec::new();
        let sales = if !purchase || dual {
            let (context, scope) = SalesAccess::new(self.db.clone(), self.rbac.clone())
                .resolve_source(actor, &entry, executor)
                .await?;
            versions.push(context.scope_version);
            scope
        } else {
            SalesReadScope::default()
        };
        let (purchase_scope, settlement) = self.purchase_sources(actor, &entry, purchase, executor).await?;
        if let Some(scope) = &purchase_scope {
            versions.push(scope.version().unwrap_or_default());
        }
        if let Some(scope) = &settlement {
            versions.push(scope.access.version());
        }
        let ledger_read = ledger_readable(&self.db, &self.rbac, actor, executor).await?;
        let mut hash = DefaultHasher::new();
        versions.hash(&mut hash);
        ledger_read.hash(&mut hash);
        let whole = if let Some(collection) = whole_collection(resource) {
            WholeAccess::query(&self.db, &self.rbac, actor, &entry, collection, executor).await?
        } else {
            WholeAccess::default()
        };
        whole.version().hash(&mut hash);
        let context = FundsReadContext {
            resource: resource.into(),
            scope_version: format!("{:x}", hash.finish()),
            as_of: Instant::now(),
        };
        let authorization = FundsAuthorization {
            whole,
            sales,
            ledger_read,
            settlement,
            purchase_scope,
            context: context.clone(),
        };
        authorization.validate(executor).await?;
        Ok((context, authorization))
    }
    async fn purchase_sources(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        purchase: bool,
        executor: &mut dyn Executor,
    ) -> Result<(Option<PurchaseReadScope>, Option<SourceQuery>)> {
        if purchase {
            let access = source_query(
                &self.db,
                &self.rbac,
                actor,
                entry,
                Arc::new(PurchaseMapping::new("purchase_order")?),
                executor,
            )
            .await?;
            let scope = PurchaseReadScope::from_compiled(access.access, access.service);
            let settlement = source_query(
                &self.db,
                &self.rbac,
                actor,
                entry,
                Arc::new(SettlementMapping::new()?),
                executor,
            )
            .await?;
            Ok((Some(scope), Some(settlement)))
        } else {
            Ok((None, None))
        }
    }
}

impl FundsAuthorization {
    /// 仓储在执行完整授权查询之前及交付之后复验全部来源凭证。
    /// # 参数
    /// executor 为原读取阶段执行器。
    /// # 返回
    /// 所有授权与映射事实保持当前时成功。
    /// # 错误
    /// 账号、权限、有效期、映射或身份设施失效时失败。
    pub(super) async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.whole.validate(executor).await?;
        if !self.sales.is_empty() {
            self.sales.validate(executor).await?;
        }
        if let Some(scope) = &self.purchase_scope {
            scope.validate(executor).await?;
        }
        if let Some(scope) = &self.settlement {
            scope.validate(executor).await?;
        }
        Ok(())
    }
}

/// 读取整账功能资格；功能资格不替代任何来源对象授权。
/// # 参数
/// db/rbac 为身份设施，actor 为账号，executor 为当前执行器。
/// # 返回
/// identity 判断的单个功能资格。
/// # 错误
/// 账号、策略或数据库读取失败保留原错误。
pub(crate) async fn ledger_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let service = AuthorizationService::new(db.clone(), rbac.clone(), AuthorizationRegistry::new(vec![])?);
    Ok(service.has_permission(actor, &Permission::parse(FINANCE_LEDGER_READ)?, executor).await?)
}

fn whole_collection(resource: &str) -> Option<&'static str> {
    match resource {
        "customer_receipt" => Some("customer_receipts"),
        "supplier_payment" => Some("supplier_payments"),
        "invoice" => Some("invoices"),
        _ => None,
    }
}
