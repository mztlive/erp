//! 组合根只装配业务事实；权限检查及查询编译统一交给身份域。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_supplier::{SupplierAccess, SupplierDataScopePort, SupplierResolvedScope, SupplierService};
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support;
map_identity_error!(erp_supplier);

#[derive(Clone)]
pub struct MongoSupplierDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoSupplierDataScope {
    /// 绑定身份设施。
    /// # 参数
    /// db、rbac 为应用共享设施。
    /// # 返回
    /// 不执行I/O的adapter。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 包装为领域授权端口。
    /// # 参数
    /// db、rbac 为共享身份设施。
    /// # 返回
    /// 已装配端口。
    /// # 错误
    /// 不执行I/O。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn SupplierDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl SupplierDataScopePort for MongoSupplierDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<SupplierResolvedScope> {
        erp_supplier::authorization::resolve(&self.db, self.rbac.clone(), actor, "supplier", action, executor)
            .await
            .map_err(map_identity_error)
    }
    async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<()> {
        erp_supplier::authorization::require(
            &self.db,
            self.rbac.clone(),
            actor,
            "supplier",
            action,
            id,
            executor,
        )
        .await
        .map_err(map_identity_error)
    }
    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supplier::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}
/// 构造绑定身份数据库的供应商访问器。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回未缓存授权的供应商访问器。
///
/// # 错误
/// 无。
pub fn supplier_access(db: Database, rbac: SharedRbacService) -> SupplierAccess {
    SupplierAccess::new(db.clone(), MongoSupplierDataScope::shared(db, rbac))
}

/// 构造已接入身份域公共解析器的供应商服务。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析供应商范围的服务。
///
/// # 错误
/// 无。
pub fn scoped_supplier_service(db: Database, rbac: SharedRbacService) -> SupplierService {
    SupplierService::new(db.clone(), super::MongoSupplierPartyFacts::shared(db.clone())).with_scope(
        MongoSupplierDataScope::shared(db.clone(), rbac),
        super::supplier::MongoSupplierAccountFacts::shared(db),
    )
}

/// 构造可签发敏感字段短时揭示令牌且已接入范围的供应商服务。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
/// * `sensitive_data` - 敏感字段编解码
///
/// # 返回
/// 返回可解析供应商范围并签发揭示令牌的服务。
///
/// # 错误
/// 无。
pub fn scoped_supplier_service_with_sensitive(
    db: Database,
    rbac: SharedRbacService,
    sensitive_data: Arc<erp_party::SensitiveDataCodec>,
) -> SupplierService {
    SupplierService::with_sensitive_data(
        db.clone(),
        super::MongoSupplierPartyFacts::shared(db.clone()),
        super::MongoSupplierSensitiveTokens::shared(sensitive_data),
    )
    .with_scope(
        MongoSupplierDataScope::shared(db.clone(), rbac),
        super::supplier::MongoSupplierAccountFacts::shared(db),
    )
}
