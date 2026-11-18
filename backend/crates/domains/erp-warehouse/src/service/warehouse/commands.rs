//! 仓库命令步骤沿用入口事务，授权先于身份与业务读取。
use application_core::AuditActor;
use erp_core::ids::{WarehouseId, WarehouseRevisionId};
use erp_identity::AccessTarget;
use erp_permissions::warehouse;
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::{
    CreateWarehouseRequest, HandlerDuty, Result, UpdateWarehouseFulfillmentHandlersRequest,
    UpdateWarehouseRequest, UpdateWarehouseSkuPolicyRequest, WarehouseRevisionInput, WarehouseService,
    WarehouseSkuPolicyView, WarehouseView, build_warehouse_revision, ensure_expected_version,
};
use crate::entity::warehouse::status::EnableStatus;
use crate::entity::warehouse::warehouse_entity::{Warehouse, WarehouseData, WarehouseUpdate};
use crate::entity::warehouse::warehouse_revision::WarehouseRevision;
use crate::entity::warehouse::warehouse_sku_policy::WarehouseSkuPolicyUpdate;
use crate::error::Error;
use crate::repository::WarehouseExt;

impl WarehouseService {
    /// 在原事务授权并创建仓库身份与首修订。
    /// # 参数
    /// actor、req 为操作人和输入，executor 为入口事务。
    /// # 返回
    /// 已创建的仓库视图。
    /// # 错误
    /// 授权、资格、业务校验或持久化失败时终止。
    pub(super) async fn create(
        &self,
        actor: &AuditActor,
        req: CreateWarehouseRequest,
        executor: &mut dyn Executor,
    ) -> Result<WarehouseView> {
        req.validate()?;
        self.authorization.require(actor, &warehouse::CREATE, AccessTarget::Operation, executor).await?;
        self.ensure_handlers(&req.inbound_handler_user_id, &req.outbound_handler_user_id, executor).await?;
        let id = WarehouseId::new(next_id());
        let mut warehouse = Warehouse::new(
            id.clone(),
            WarehouseData::new(req.warehouse_code)
                .with_status(req.status.unwrap_or(EnableStatus::Active))
                .with_inbound_handler_user_id(req.inbound_handler_user_id)
                .with_outbound_handler_user_id(req.outbound_handler_user_id),
            actor.id(),
        )?;
        let revision = self.revision(
            id,
            1,
            WarehouseRevisionInput {
                name: req.name,
                region_code: req.region_code,
                address: req.address,
                contact: req.contact,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
                change_reason: req.change_reason,
            },
        )?;
        self.db.warehouse().create_warehouse_with_revision(&mut warehouse, &revision, executor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "warehouse.create",
            "warehouse",
            warehouse.base.id.clone(),
        )?;
        self.audit.persist(&audit, executor).await?;
        Ok(warehouse.into())
    }

    /// 原对象、修订序号及写入均在授权后的同一快照读取。
    /// # 参数
    /// actor、id、req 为原操作目标，executor 为入口事务。
    /// # 返回
    /// 已更新的仓库视图。
    /// # 错误
    /// 无资格、对象缺失、版本冲突或持久化失败时终止。
    pub(super) async fn update(
        &self,
        actor: &AuditActor,
        id: &str,
        req: UpdateWarehouseRequest,
        executor: &mut dyn Executor,
    ) -> Result<WarehouseView> {
        req.validate()?;
        let mut warehouse = self
            .load_warehouse_for_update(
                actor,
                id,
                req.version,
                &req.inbound_handler_user_id,
                &req.outbound_handler_user_id,
                executor,
            )
            .await?;
        let revision_no = self.db.warehouse().next_revision_no(id, executor).await?;
        warehouse.update(
            WarehouseUpdate {
                status: Some(req.status),
                inbound_handler_user_id: Some(Some(req.inbound_handler_user_id)),
                outbound_handler_user_id: Some(Some(req.outbound_handler_user_id)),
            },
            actor.id(),
        )?;
        let revision = self.revision(
            WarehouseId::new(id.to_owned()),
            revision_no,
            WarehouseRevisionInput {
                name: req.name,
                region_code: req.region_code,
                address: req.address,
                contact: req.contact,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
                change_reason: req.change_reason,
            },
        )?;
        warehouse.apply_revision(&revision)?;
        self.db.warehouse_revisions().create(&revision, executor).await?;
        self.db.warehouses().update(&mut warehouse, executor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "warehouse.update",
            "warehouse",
            warehouse.base.id.clone(),
        )?;
        self.audit.persist(&audit, executor).await?;
        Ok(warehouse.into())
    }

    /// 经办人资格和仓库版本沿用原事务，不在授权前预读。
    /// # 参数
    /// actor、id、req 为原操作目标，executor 为入口事务。
    /// # 返回
    /// 配置生效后的仓库视图。
    /// # 错误
    /// 无资格、对象缺失、版本冲突或持久化失败时终止。
    pub(super) async fn update_handlers(
        &self,
        actor: &AuditActor,
        id: &str,
        req: UpdateWarehouseFulfillmentHandlersRequest,
        executor: &mut dyn Executor,
    ) -> Result<WarehouseView> {
        req.validate()?;
        let mut warehouse = self
            .load_warehouse_for_update(
                actor,
                id,
                req.version,
                &req.inbound_handler_user_id,
                &req.outbound_handler_user_id,
                executor,
            )
            .await?;
        let audit_message = format!(
            "inbound:{}->{};outbound:{}->{}",
            warehouse.inbound_handler_user_id.as_deref().unwrap_or("未配置"),
            req.inbound_handler_user_id,
            warehouse.outbound_handler_user_id.as_deref().unwrap_or("未配置"),
            req.outbound_handler_user_id,
        );
        warehouse.update(
            WarehouseUpdate {
                status: None,
                inbound_handler_user_id: Some(Some(req.inbound_handler_user_id)),
                outbound_handler_user_id: Some(Some(req.outbound_handler_user_id)),
            },
            actor.id(),
        )?;
        let audit = self.audit.resource_log_with_message(
            actor.clone(),
            "warehouse.fulfillment_handlers.update",
            "warehouse",
            warehouse.base.id.clone(),
            Some(audit_message),
        )?;
        self.db.warehouses().update(&mut warehouse, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(warehouse.into())
    }

    /// 授权后在原快照检查策略版本和完整重叠集合。
    /// # 参数
    /// actor、id、req 为原操作目标，executor 为入口事务。
    /// # 返回
    /// 已更新的策略视图。
    /// # 错误
    /// 无资格、对象缺失、版本冲突、区间重叠或持久化失败时终止。
    pub(super) async fn update_policy(
        &self,
        actor: &AuditActor,
        id: &str,
        req: UpdateWarehouseSkuPolicyRequest,
        executor: &mut dyn Executor,
    ) -> Result<WarehouseSkuPolicyView> {
        req.validate()?;
        self.authorization
            .require(actor, &warehouse::SKU_POLICY_UPDATE, AccessTarget::Operation, executor)
            .await?;
        let mut policy = self
            .db
            .warehouse_sku_policies()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("预警策略不存在".into()))?;
        ensure_expected_version(policy.base.version, req.version)?;
        policy.update(WarehouseSkuPolicyUpdate {
            minimum_available_quantity: req.minimum_available_quantity,
            status: req.status,
        })?;
        let existing = self
            .db
            .warehouse()
            .sku_policies_for_dimensions(&policy.warehouse_id, &policy.sku_id, executor)
            .await?;
        policy.ensure_no_overlap(&existing).map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "warehouse_sku_policy.update",
            "warehouse_sku_policy",
            policy.base.id.clone(),
        )?;
        self.db.warehouse_sku_policies().update(&mut policy, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(policy.into())
    }

    /// 授权、真实策略读取、CAS 删除和审计共用原事务。
    /// # 参数
    /// actor 为操作人，id 为策略身份，executor 为入口事务。
    /// # 返回
    /// 策略及审计写入成功时完成。
    /// # 错误
    /// 无资格、策略缺失、并发冲突或持久化失败时终止。
    pub(super) async fn delete_policy(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .require(actor, &warehouse::SKU_POLICY_DELETE, AccessTarget::Operation, executor)
            .await?;
        let mut policy = self
            .db
            .warehouse_sku_policies()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("预警策略不存在".into()))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "warehouse_sku_policy.delete",
            "warehouse_sku_policy",
            policy.base.id.clone(),
        )?;
        self.db.warehouse_sku_policies().soft_delete(&mut policy, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(())
    }

    /// 按既定顺序校验操作资格、经办人及仓库版本。
    async fn load_warehouse_for_update(
        &self,
        actor: &AuditActor,
        id: &str,
        expected_version: u64,
        inbound: &str,
        outbound: &str,
        executor: &mut dyn Executor,
    ) -> Result<Warehouse> {
        self.authorization.require(actor, &warehouse::UPDATE, AccessTarget::Operation, executor).await?;
        self.ensure_handlers(inbound, outbound, executor).await?;
        let warehouse = self
            .db
            .warehouses()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("仓库不存在".into()))?;
        ensure_expected_version(warehouse.base.version, expected_version)?;
        Ok(warehouse)
    }

    async fn ensure_handlers(
        &self,
        inbound: &str,
        outbound: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_handler_eligible(inbound, HandlerDuty::Inbound, executor).await?;
        self.ensure_handler_eligible(outbound, HandlerDuty::Outbound, executor).await
    }

    /// 为本次业务写入构造独立修订，不访问数据库。
    fn revision(
        &self,
        id: WarehouseId,
        revision_no: u32,
        input: WarehouseRevisionInput,
    ) -> Result<WarehouseRevision> {
        build_warehouse_revision(
            id,
            WarehouseRevisionId::new(next_id()),
            revision_no,
            input,
            self.fingerprint.as_ref(),
            self.fingerprint_key,
        )
    }
}

#[cfg(test)]
mod tests;
