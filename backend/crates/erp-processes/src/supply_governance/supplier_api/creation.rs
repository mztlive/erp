use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supplier::SupplierExt;
use erp_supply::dto::supplier_api::*;
use erp_supply::entity::supplier_api::PreparedSupplierConnectionCreate;
use erp_supply::service::supplier_api::{SupplierApiService, map_command_shape_rejection};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierApiGovernanceProcess;
use crate::{Error, Result};
impl SupplierApiGovernanceProcess {
    /// 创建供应商 API 连接及其能力声明（跨集合事务写入）。
    ///
    /// 一个事务内写入 `supplier_api_connections`、`supplier_api_capabilities` 与
    /// 审计日志，保证「连接配置 + 能力清单」原子可见（数据模型 §6.14）。
    /// 唯一性（`connection_code`、`(connection_id, capability_code)`）由唯一索引
    /// 透出 `DuplicateKey` → 409，不做「先查后插」。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建连接的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - API 供应商不存在
    /// * `ConflictError` - 连接代码重复
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_connection(
        &self,
        req: CreateSupplierApiConnectionRequest,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionView> {
        req.validate()?;
        let prepared = PreparedSupplierConnectionCreate::try_new(
            req.endpoint_reference.as_deref(),
            req.credential_reference.as_deref(),
            req.status,
            req.capabilities.len(),
        )
        .map_err(map_command_shape_rejection)?;
        self.db
            .supplier_accounts()
            .find_by_id(&req.supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("API 供应商不存在".to_string()))?;

        let connection = SupplierApiService::prepare_connection(req, prepared, actor.id())?;
        let audit = actor.clone().resource_log(
            "supplier_api_connection.create",
            "supplier_api_connection",
            connection.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let connection_tx = connection.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    SupplierApiService::new(db.clone())
                        .persist_created_connection(&connection_tx, executor)
                        .await?;
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;

        Ok(connection.into())
    }
}
