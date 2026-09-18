//! 域 D24 供应商供给服务。
//!
//! 公司 SKU 是唯一商品主数据。服务只编排“公司 SKU → 供应商供给”：新增供给时
//! 原子写入稳定身份、首版商业条款、实时可供投影、审计与幂等结果；改价只追加
//! 商业条款修订；库存与可供状态只更新独立投影。

//! 供给单域命令：身份、商业修订、实时可供及命令重放。
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::dto::supplier_offering as dto;
use crate::dto::supplier_offering::{
    CREATE_OFFERING_COMMAND, CreateSupplierOfferingRequest, CreateSupplierOfferingResult,
    REVISE_OFFERING_COMMAND, ReviseSupplierOfferingRequest, ReviseSupplierOfferingResult,
    UPDATE_OFFERING_AVAILABILITY_COMMAND, UpdateSupplierOfferingAvailabilityRequest,
    UpdateSupplierOfferingAvailabilityResult,
};
use crate::entity::supplier_offering::{
    OfferingStatus, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingCommand,
    SupplierOfferingRevision,
};
use crate::ports::offering_qualification::QualificationPort;
use crate::ports::{FailClosedOfferingDataScopePort, OfferingDataScopePort};
use crate::repository::prelude::*;
use crate::repository::{SupplierApiExt, SupplierOfferingExt};
use crate::{Error, Result};
mod access;
mod handover;
mod recovery;
mod write;

pub use access::{OfferingAccess, offering_scope};

/// 单域命令准备结果；回放不继续读取当前事实。
pub enum CommandPreparation<P, R> {
    Replay(R),
    Apply(P),
}
/// 创建供给的已校验事实；command在原事务前构造。
pub struct PreparedCreate {
    pub offering: SupplierOffering,
    revision: SupplierOfferingRevision,
    availability: SupplierOfferingAvailability,
    command: SupplierOfferingCommand,
    pub result: CreateSupplierOfferingResult,
    pub fingerprint: String,
}
/// 追加商业修订的已校验事实；command仍在CAS成功后构造。
pub struct PreparedRevision {
    pub offering: SupplierOffering,
    revision: SupplierOfferingRevision,
    next_no: u32,
    next_status: OfferingStatus,
    expected_version: u64,
    idempotency_key: String,
    pub fingerprint: String,
}
/// 覆盖实时可供事实；不读取资格、不改变商业条款。
pub struct PreparedAvailability {
    availability: SupplierOfferingAvailability,
    offering_id: SupplierOfferingId,
    result_version: u64,
    idempotency_key: String,
    pub fingerprint: String,
}
/// 只持有本域Database；外部资格由消费方Port提供。
pub struct SupplierOfferingService {
    db: Database,
    data_scope: std::sync::Arc<dyn OfferingDataScopePort>,
}
impl SupplierOfferingService {
    /// 创建供应商供给服务。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回服务实例；范围 Port 缺省失败关闭。
    pub fn new(db: Database) -> Self {
        Self { db, data_scope: FailClosedOfferingDataScopePort::shared() }
    }

    /// 注入供给范围 Port。
    ///
    /// # 参数
    /// * `data_scope` - 组合层装配的公共解析 adapter
    ///
    /// # 返回
    /// 返回绑定范围 Port 的服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// HTTP 与写命令必须注入生产 adapter，不得保留失败关闭端口。
    pub fn with_data_scope(mut self, data_scope: std::sync::Arc<dyn OfferingDataScopePort>) -> Self {
        self.data_scope = data_scope;
        self
    }

    /// 构造本域范围访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库与范围 Port 的访问器。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn access(&self) -> OfferingAccess {
        OfferingAccess::new(self.db.clone(), self.data_scope.clone())
    }
    /// 按原validate/replay/identity/source/terms/qualification顺序准备创建。
    /// 回放返回原结果；任何错误停止后续读取，执行器由调用方决定。
    pub async fn prepare_create<P: QualificationPort + ?Sized>(
        &self,
        req: &CreateSupplierOfferingRequest,
        actor: &application_core::AuditActor,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<CommandPreparation<PreparedCreate, CreateSupplierOfferingResult>, P::Error> {
        req.validate()?;
        let fingerprint = req.command_fingerprint()?;
        if let Some(command) = self.command_record(&req.idempotency_key, executor).await? {
            command
                .ensure_replayable(CREATE_OFFERING_COMMAND, &fingerprint)
                .map_err(|e| Error::ConflictError(e.to_string()))?;
            return command
                .replay_result()
                .map(CommandPreparation::Replay)
                .map_err(|e| Error::Internal(e.to_string()).into());
        }
        let (maintainer, org) = self.access().maintainer_org(None, actor, executor).await?;
        self.access().ensure_writable(actor, "create", &maintainer, &org, executor).await?;
        let offering_id = SupplierOfferingId::new(next_id());
        let mut offering = SupplierOffering::new(
            offering_id.clone(),
            req.try_into_offering_data(maintainer, org)?,
            actor.id(),
        )?;
        self.ensure_identity_available(&offering, executor).await?;
        self.ensure_source_connection(&offering, executor).await?;
        let revision_data = req.terms.try_into_revision_data(offering_id.clone(), 1)?;
        let revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new(next_id()), revision_data)?;
        qualification
            .ensure_qualified(&offering.supplier_id, &offering.sku_id, revision.valid_from, executor)
            .await?;
        let received_at = Instant::now();
        let source_updated_at = dto::resolve_source_updated_at(req.source_updated_at, received_at);
        let availability_data = req.try_into_availability_data(
            offering_id.clone(),
            source_updated_at,
            received_at,
            actor.id().to_string(),
        )?;
        let availability = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new(next_id()),
            availability_data,
        )?;
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        let result = CreateSupplierOfferingResult {
            offering_id: offering.base.id.clone(),
            revision_id: revision.base.id.clone(),
            availability_id: availability.base.id.clone(),
            revision_no: 1,
            status: offering.stable.status,
        };
        let command = SupplierOfferingCommand::with_result(
            next_id(),
            &req.idempotency_key,
            CREATE_OFFERING_COMMAND,
            &fingerprint,
            &result,
        )?;
        Ok(CommandPreparation::Apply(PreparedCreate {
            offering,
            revision,
            availability,
            command,
            result,
            fingerprint,
        }))
    }
    /// 同一执行器写入供给、首版、可供与原命令；审计由process随后写入。
    pub async fn persist_created(
        &self,
        prepared: &PreparedCreate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        write::created(
            &crate::repository::supplier_offering::write::MongoOfferingWrite::new(&self.db),
            prepared,
            executor,
        )
        .await
    }
    /// 加载最大修订并构造新版本；仅Active目标状态读取资格。
    pub async fn prepare_revise<P: QualificationPort + ?Sized>(
        &self,
        id: &str,
        req: &ReviseSupplierOfferingRequest,
        actor: &application_core::AuditActor,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<CommandPreparation<PreparedRevision, ReviseSupplierOfferingResult>, P::Error>
    {
        req.validate()?;
        let fingerprint = req.command_fingerprint(id)?;
        if let Some(command) = self.command_record(&req.idempotency_key, executor).await? {
            command
                .ensure_replayable(REVISE_OFFERING_COMMAND, &fingerprint)
                .map_err(|e| Error::ConflictError(e.to_string()))?;
            return command
                .replay_result()
                .map(CommandPreparation::Replay)
                .map_err(|e| Error::Internal(e.to_string()).into());
        }
        let mut offering = self.access().require_offering(actor, "update", id, executor).await?;
        if !offering.has_responsibility() {
            return Err(Error::BusinessLogicError("供给缺少维护人或主属组织，请先交接".into()).into());
        }
        let current_no = self.current_revision_no(&offering, executor).await?;
        let next_no = offering
            .next_revision_no(current_no, req.expected_revision_no)
            .map_err(|_| Error::ConflictError("供给版本已经变化，请刷新后重新保存".to_string()))?;
        let revision_data =
            req.terms.try_into_revision_data(SupplierOfferingId::new(offering.base.id.clone()), next_no)?;
        let revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new(next_id()), revision_data)?;
        let next_status = req.status.unwrap_or(offering.stable.status);
        if next_status == OfferingStatus::Active {
            qualification
                .ensure_qualified(&offering.supplier_id, &offering.sku_id, revision.valid_from, executor)
                .await?;
        }
        offering.update_status(next_status, actor.id())?;
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        let expected_version = offering.next_persisted_version()?;
        Ok(CommandPreparation::Apply(PreparedRevision {
            offering,
            revision,
            next_no,
            next_status,
            expected_version,
            idempotency_key: req.idempotency_key.clone(),
            fingerprint,
        }))
    }
    /// 新修订insert、供给CAS后构造原响应与命令并写入；不另开事务。
    pub async fn persist_revised(
        &self,
        prepared: &mut PreparedRevision,
        executor: &mut dyn Executor,
    ) -> Result<ReviseSupplierOfferingResult> {
        write::revised(
            &crate::repository::supplier_offering::write::MongoOfferingWrite::new(&self.db),
            prepared,
            executor,
        )
        .await
    }
    /// 按原投影版本、来源时间和数量顺序准备可供更新。
    pub async fn prepare_availability(
        &self,
        id: &str,
        req: &UpdateSupplierOfferingAvailabilityRequest,
        actor: &application_core::AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CommandPreparation<PreparedAvailability, UpdateSupplierOfferingAvailabilityResult>> {
        req.validate()?;
        let fingerprint = req.command_fingerprint(id)?;
        if let Some(command) = self.command_record(&req.idempotency_key, executor).await? {
            command
                .ensure_replayable(UPDATE_OFFERING_AVAILABILITY_COMMAND, &fingerprint)
                .map_err(|e| Error::ConflictError(e.to_string()))?;
            return command
                .replay_result()
                .map(CommandPreparation::Replay)
                .map_err(|e| Error::Internal(e.to_string()));
        }
        let offering = self.access().require_offering(actor, "update", id, executor).await?;
        if !offering.has_responsibility() {
            return Err(Error::BusinessLogicError("供给缺少维护人或主属组织，请先交接".into()));
        }
        let offering_id = SupplierOfferingId::new(id.trim());
        let mut availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&offering_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供给可供状态不存在".to_string()))?;
        if let Some(expected_version) = req.expected_version {
            availability
                .ensure_version(expected_version)
                .map_err(|_| Error::ConflictError("可供状态已经变化，请刷新后重新保存".to_string()))?;
        }
        let received_at = Instant::now();
        let source_updated_at = dto::resolve_source_updated_at(req.source_updated_at, received_at);
        let availability_data = req.try_into_availability_data(
            offering_id.clone(),
            source_updated_at,
            received_at,
            actor.id().to_string(),
        )?;
        availability.apply(availability_data)?;
        let result_version = availability.next_persisted_version()?;
        Ok(CommandPreparation::Apply(PreparedAvailability {
            availability,
            offering_id,
            result_version,
            idempotency_key: req.idempotency_key.clone(),
            fingerprint,
        }))
    }
    /// 可供CAS后构造原响应与命令并写入，保留命令ID生成时点。
    pub async fn persist_availability(
        &self,
        prepared: &mut PreparedAvailability,
        executor: &mut dyn Executor,
    ) -> Result<UpdateSupplierOfferingAvailabilityResult> {
        write::availability(
            &crate::repository::supplier_offering::write::MongoOfferingWrite::new(&self.db),
            prepared,
            executor,
        )
        .await
    }
    async fn ensure_identity_available(
        &self,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let existing = self
            .db
            .supplier_offerings()
            .find_by_supplier_identity(&offering.supplier_id, &offering.supplier_sku_code, executor)
            .await?;
        if existing.is_some() {
            return Err(Error::ConflictError("该供应商 SKU 已登记供给".to_string()));
        }
        Ok(())
    }
    async fn ensure_source_connection(
        &self,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let Some(connection_id) = offering.source_connection_id.as_ref() else {
            return Ok(());
        };
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(connection_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商 API 连接不存在".to_string()))?;
        if connection.supplier_id != offering.supplier_id || !connection.is_active() {
            return Err(Error::BusinessLogicError("供应商 API 连接不属于该供应商或未启用".to_string()));
        }
        Ok(())
    }
    async fn current_revision_no(
        &self,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        self.db
            .supplier_offering_revisions()
            .current_revision_no(&SupplierOfferingId::new(offering.base.id.clone()), executor)
            .await
            .map_err(Into::into)
    }
    async fn command_record(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingCommand>> {
        self.db
            .supplier_offering_commands()
            .find_by_idempotency_key(idempotency_key, executor)
            .await
            .map_err(Into::into)
    }
    /// 回收任意事务错误后读取原命令；重读失败优先于原事务错误。
    pub async fn resolve_command_result<T, E>(
        &self,
        transaction_result: std::result::Result<(), E>,
        intended_result: T,
        idempotency_key: &str,
        operation: &str,
        fingerprint: &str,
        executor: &mut dyn Executor,
    ) -> std::result::Result<T, E>
    where
        T: DeserializeOwned,
        E: From<Error>,
    {
        recovery::resolve(
            self,
            transaction_result.map(|()| intended_result),
            idempotency_key,
            operation,
            fingerprint,
            executor,
        )
        .await
    }
    /// 回收任意事务错误后读取原命令；重读失败优先于原事务错误。
    pub async fn resolve_written_result<T, E>(
        &self,
        transaction_result: std::result::Result<T, E>,
        idempotency_key: &str,
        operation: &str,
        fingerprint: &str,
        executor: &mut dyn Executor,
    ) -> std::result::Result<T, E>
    where
        T: DeserializeOwned,
        E: From<Error>,
    {
        recovery::resolve(self, transaction_result, idempotency_key, operation, fingerprint, executor).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::supplier_offering::{
        AvailabilityStatus, OfferingSourceType, SupplierOfferingCommandData,
    };
    /// 覆盖存量命令重放：历史裸指纹命令与 DTO 新指纹同键同载荷可重放，跨操作必须冲突。
    #[test]
    fn stored_command_replays_dto_fingerprint_and_decodes_result() {
        let req = create_request();
        let fingerprint = req.command_fingerprint().unwrap();
        let stored = SupplierOfferingCommand::new(
            "command-1",
            SupplierOfferingCommandData {
                idempotency_key: req.idempotency_key.clone(),
                operation: CREATE_OFFERING_COMMAND.to_string(),
                request_fingerprint: fingerprint.clone(),
                result_json: "{\"offering_id\":\"offering-1\",\"revision_id\":\"revision-1\",\
                              \"availability_id\":\"availability-1\",\"revision_no\":1,\
                              \"status\":\"ACTIVE\"}"
                    .to_string(),
            },
        )
        .unwrap();
        stored.ensure_replayable(CREATE_OFFERING_COMMAND, &fingerprint).unwrap();
        assert!(stored.ensure_replayable(REVISE_OFFERING_COMMAND, &fingerprint).is_err());
        let result: CreateSupplierOfferingResult = stored.replay_result().unwrap();
        assert_eq!(result.offering_id, "offering-1");
        assert_eq!(result.revision_no, 1);
        assert_eq!(result.status, OfferingStatus::Active);
    }
    fn create_request() -> CreateSupplierOfferingRequest {
        CreateSupplierOfferingRequest {
            sku_id: "sku-1".to_string(),
            supplier_id: "supplier-1".to_string(),
            supplier_product_code: Some("P-1".to_string()),
            supplier_sku_code: "SKU-1".to_string(),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
            terms: crate::dto::supplier_offering::SupplierOfferingTermsWrite {
                dropship_supply_price_gross: "10.00".to_string(),
                bulk_supply_price_gross: "9.00".to_string(),
                input_tax_rate: "0.13".to_string(),
                bulk_minimum_order_quantity: "10".to_string(),
                supply_region: vec!["CN".to_string()],
                product_capabilities: vec!["DROP_SHIP".to_string()],
                valid_from: "2026-01-01".to_string(),
                valid_to: None,
                dropship_express: None,
                freight_amount: None,
                service_fee_amount: None,
            },
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some("100".to_string()),
            source_updated_at: Some(1_700_000_000),
            source_revision_token: None,
            change_reason: "登记新供给".to_string(),
            idempotency_key: "key-1".to_string(),
        }
    }
    #[test]
    fn write_paths_use_dto_try_into_data_with_money_precision() {
        use erp_core::ids::{SupplierOfferingId, SupplierOfferingRevisionId};

        use crate::entity::supplier_offering::SupplierOfferingRevision;
        let req = create_request();
        let data = req.terms.try_into_revision_data(SupplierOfferingId::new("offering-1"), 1).unwrap();
        let revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("revision-1"), data).unwrap();
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.dropship_supply_price_gross.to_string(), "10.00");
        assert_eq!(revision.bulk_minimum_order_quantity.to_string(), "10");
        assert_eq!(revision.valid_from.to_string(), "2026-01-01");
    }
}
