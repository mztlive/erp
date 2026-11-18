//! 采购窄端口只转交固定入口和真实目标；政策由共享统一授权服务执行。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, resources};
use mongodb::Database;
use persistence_core::Executor;

use super::{PurchaseDataScopePort, PurchaseResolvedScope};
use crate::authorization::PurchaseFacts;
use crate::entity::purchase_order::PurchaseOrder;
use crate::error::{Error, Result};

/// 绑定进程共享授权服务及采购筛选所需的组织事实读取。
#[derive(Clone)]
pub struct PurchaseDataScope {
    directory: OrganizationDirectoryService,
    authorization: Arc<AuthorizationService>,
}

impl PurchaseDataScope {
    /// 注入已完成全部业务绑定的共享授权服务。不执行 I/O、也不构造授权政策的采购端口。
    ///
    /// db 为业务数据库，authorization 为组合根注入的共享统一授权服务。
    pub fn shared(db: Database, authorization: Arc<AuthorizationService>) -> Arc<dyn PurchaseDataScopePort> {
        Arc::new(Self { directory: OrganizationDirectoryService::new(db), authorization })
    }
}

#[async_trait]
impl PurchaseDataScopePort for PurchaseDataScope {
    async fn require_record_entry(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        purchase_entry(entry)?;
        self.authorization.require(actor, entry, AccessTarget::record(id)?, executor).await?;
        Ok(())
    }

    async fn require_entry(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        purchase_entry(entry)?;
        self.authorization
            .require(actor, entry, AccessTarget::Candidate(PurchaseFacts::candidate(order)), executor)
            .await?;
        Ok(())
    }

    async fn resolve(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseResolvedScope> {
        purchase_entry(entry)?;
        let compiled = Arc::new(self.authorization.query(actor, entry, executor).await?);
        Ok(PurchaseResolvedScope {
            scope_version: compiled.version(),
            policy_version: compiled.policy_version(),
            as_of: compiled.as_of(),
            compiled,
            authorization: self.authorization.clone(),
        })
    }

    async fn resolve_entry(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseResolvedScope> {
        self.resolve(actor, entry, executor).await
    }

    async fn expand_org_units(
        &self,
        ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Ok(self.directory.expand_org_units(ids, include_descendants, executor).await?)
    }

    async fn org_member_ids(
        &self,
        ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(self.directory.org_member_ids(ids, at, executor).await?)
    }

    async fn own_org(&self, user: &str, at: Instant, executor: &mut dyn Executor) -> Result<Option<String>> {
        Ok(self.directory.own_org(user, at, executor).await?)
    }
}

// 采购端口拒绝其他根资源，防止同名身份被用于另一领域的授权。
fn purchase_entry(entry: &Permission) -> Result<()> {
    if entry.resource() != resources::PURCHASE_ORDER {
        return Err(Error::ValidationError("采购端口只接受采购单入口".into()));
    }
    Ok(())
}
