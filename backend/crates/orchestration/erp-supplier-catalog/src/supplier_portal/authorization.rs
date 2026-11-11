//! 精确申请对象授权供工作项、内部读取与命令统一复用。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::SharedRbacService;
use erp_read_models::supplier_portal::PortalReadAuthorizationPort;
use erp_read_models::{Error as ReadError, Result as ReadResult};
use erp_supplier::Error as SupplierError;
use erp_supply::Error as SupplyError;
use mongodb::Database;
use persistence_core::Executor;

use super::request_access::{RequestAction, RequestTarget, require_request};
use crate::wiring::CatalogServices;
use crate::{Error, Result};

/// 内部申请读取的对象范围适配器，不给外部账号授予内部范围。
pub struct PortalReadAuthorization {
    services: Arc<CatalogServices>,
    db: Database,
    rbac: SharedRbacService,
}
impl PortalReadAuthorization {
    /// 装配内部门户申请读权限。
    /// # 参数
    /// 当前数据库与RBAC快照。
    /// # 返回
    /// 返回未缓存对象授权的适配器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService, services: Arc<CatalogServices>) -> Self {
        Self { db, rbac, services }
    }
}
#[async_trait]
impl PortalReadAuthorizationPort for PortalReadAuthorization {
    async fn offering_readable(
        &self,
        actor: &AuditActor,
        offering_id: &str,
        executor: &mut dyn Executor,
    ) -> ReadResult<bool> {
        match self.services.offering_access.require_offering(actor, "list", offering_id, executor).await {
            Ok(_) => Ok(true),
            Err(SupplyError::NotFound(_) | SupplyError::Forbidden(_)) => Ok(false),
            Err(error) => Err(map_read_error(error.into())),
        }
    }
    async fn request_readable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> ReadResult<bool> {
        request_readable(&self.db, &self.rbac, actor, request_id, executor).await.map_err(map_read_error)
    }
    async fn supplier_readable(
        &self,
        actor: &AuditActor,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> ReadResult<bool> {
        match self.services.suppliers.require_with(actor, "detail", supplier_id, executor).await {
            Ok(_) => Ok(true),
            Err(SupplierError::NotFound(_) | SupplierError::Forbidden(_)) => Ok(false),
            Err(error) => Err(map_read_error(error.into())),
        }
    }
}

fn map_read_error(error: Error) -> ReadError {
    match error {
        Error::Internal(v) => ReadError::Internal(v),
        Error::NotFound(v) => ReadError::NotFound(v),
        Error::ValidationError(v) => ReadError::ValidationError(v),
        Error::InvalidInput(v) => ReadError::InvalidInput(v),
        Error::BusinessLogicError(v) => ReadError::BusinessLogicError(v),
        Error::ConflictError(v) => ReadError::ConflictError(v),
        Error::Forbidden(v) => ReadError::Forbidden(v),
        Error::Unauthenticated(v) => ReadError::Unauthenticated(v),
        Error::Logic(v) => ReadError::Logic(v),
        Error::Rbac(v) => ReadError::Rbac(v),
        Error::OutcomeUnknown(v) => ReadError::OutcomeUnknown(v),
        Error::RepositoryError(v) => ReadError::RepositoryError(v),
        Error::ReceiptDuplicate(v) => ReadError::ReceiptDuplicate(v),
        Error::TransientTransaction(v) => ReadError::TransientTransaction(v),
        Error::Coded(v) => ReadError::Coded(v),
    }
}

/// 证明内部账号可读取当前精确申请对应的供应商及供给。
/// # 参数
/// 数据库、内部权限来源、真实内部账号、精确申请与调用方执行器。
/// # 返回
/// 对象在范围内为真；不存在或禁止为假。
/// # 错误
/// 授权漂移、配置或基础设施错误保持失败关闭。
pub async fn request_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(target) = RequestTarget::load(db, id, executor).await? else {
        return Ok(false);
    };
    match require_request(db, rbac, actor, target, RequestAction::Read, executor).await {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}
