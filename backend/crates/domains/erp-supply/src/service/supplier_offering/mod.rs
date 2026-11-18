//! 域 D24 供应商供给服务。
//!
//! 公司 SKU 是唯一商品主数据。服务只编排“公司 SKU → 供应商供给”：新增供给时
//! 在调用方事务内写入稳定身份、首版商业条款、实时可供投影与幂等命令；改价只追加
//! 商业条款修订；库存与可供状态只更新独立投影。

//! 供给单域命令：身份、商业修订、实时可供及命令重放。
mod history;
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierDeliveryTemplateId, SupplierOfferingId, SupplierOfferingRevisionId};
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
    OfferingSourceType, OfferingStatus, SupplierOffering, SupplierOfferingAvailability,
    SupplierOfferingCommand, SupplierOfferingRevision,
};
use crate::ports::offering_qualification::QualificationPort;
use crate::repository::prelude::*;
use crate::repository::{SupplierApiExt, SupplierOfferingExt};
use crate::{Error, Result};
mod access;
mod api_price;
mod creation;
mod handover;
mod recovery;
mod source_observation;
mod write;

pub use access::OfferingAccess;

/// 单域命令准备结果；回放已经核验当前资格与真实供给。
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
    data_scope: OfferingAccess,
}
impl SupplierOfferingService {
    /// 创建供应商供给服务。
    /// 返回服务实例；授权访问器 缺省失败关闭。
    pub fn new(db: Database) -> Self {
        Self { db: db.clone(), data_scope: OfferingAccess::unwired(db.clone()) }
    }

    /// 注入供给授权访问器。
    /// 返回绑定授权访问器 的服务。
    /// HTTP 与写命令必须注入生产授权设施，不得保留未接线访问器。
    /// `data_scope`：组合层装配的本域授权访问器。
    pub fn with_data_scope(mut self, data_scope: OfferingAccess) -> Self {
        self.data_scope = data_scope;
        self
    }

    // 构造本域范围访问器。
    // 返回绑定当前数据库与授权访问器 的访问器。
    pub(crate) fn access(&self) -> OfferingAccess {
        self.data_scope.clone()
    }
    /// 在创建供给事务中校验请求和当前授权；同键同载荷且仍可访问真实供给时返回原结果。新命令核对责任归属、
    /// 唯一 SKU、来源及资格后，仅准备供给、首版条款、实时可供和回执。
    /// 同键异载荷为冲突；回执无法解码为内部错误，不当作新命令执行。新建输入、范围、来源或资格有误时不返
    /// 回待写事实。
    pub async fn prepare_create<P: QualificationPort + ?Sized>(
        &self,
        req: &CreateSupplierOfferingRequest,
        actor: &AuditActor,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<CommandPreparation<PreparedCreate, CreateSupplierOfferingResult>, P::Error> {
        req.validate()?;
        let fingerprint = req.command_fingerprint()?;
        if let recovery::Replay::Stored(result) = recovery::created(self, req, actor, executor).await? {
            return Ok(CommandPreparation::Replay(result));
        }

        let mut offering = self.prepare_initial_offering(req, actor, executor).await?;
        let offering_id = SupplierOfferingId::new(offering.base.id.clone());
        let revision = self.prepare_initial_revision(req, &offering, executor).await?;
        qualification
            .ensure_qualified(&offering.supplier_id, &offering.sku_id, revision.valid_from, executor)
            .await?;

        let availability = Self::prepare_initial_availability(req, offering_id, actor)?;
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
    /// 重验待创建 API 供给的来源采购价后，用原执行器写入稳定供给、首版条款、实时可供和命令回执。调用方须
    /// 在同一事务完成审计；校验或写入失败向上传播。
    pub async fn persist_created(
        &self,
        prepared: &PreparedCreate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_api_create_price(&prepared.offering, &prepared.revision, executor).await?;
        write::created(&self.db, prepared, executor).await
    }
    /// 在调用方执行器内核验目标供给权限并尝试重放。新命令按最大修订号做版本检查，核对责任与来源报价，准
    /// 备追加条款；只有目标为 `Active` 才重新取供应资格。
    /// 缺责任或资格不合格时拒绝，期望修订号或幂等载荷不一致时冲突，损坏回执不得重跑。
    pub async fn prepare_revise<P: QualificationPort + ?Sized>(
        &self,
        id: &str,
        req: &ReviseSupplierOfferingRequest,
        actor: &AuditActor,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<CommandPreparation<PreparedRevision, ReviseSupplierOfferingResult>, P::Error>
    {
        req.validate()?;
        let fingerprint = req.command_fingerprint(id)?;
        let mut offering = match recovery::revised(self, id, req, actor, executor).await? {
            recovery::Replay::Stored(result) => return Ok(CommandPreparation::Replay(result)),
            recovery::Replay::Absent(offering) => {
                offering.ok_or_else(|| Error::Internal("修订命令缺少已授权供给".into()))?
            },
        };
        if !offering.has_responsibility() {
            return Err(Error::BusinessLogicError("供给缺少维护人或主属组织，请先交接".into()).into());
        }

        let current_no = self.current_revision_no(&offering, executor).await?;
        let next_no = offering
            .next_revision_no(current_no, req.expected_revision_no)
            .map_err(|_| Error::ConflictError("供给版本已经变化，请刷新后重新保存".to_string()))?;
        let revision_data = req.terms.try_into_source_revision_data(
            SupplierOfferingId::new(offering.base.id.clone()),
            next_no,
            offering.source_type,
        )?;
        let mut revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new(next_id()), revision_data)?;
        self.ensure_api_prices(&offering, &mut revision, executor).await?;

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
    /// 用原执行器追加不可变条款修订，并用 CAS 切换供给当前指针和状态。两步成功后才写入相同命令的原结果回
    /// 执；修订、CAS 或回执失败时向上传播，由调用方控制事务。
    pub async fn persist_revised(
        &self,
        prepared: &mut PreparedRevision,
        executor: &mut dyn Executor,
    ) -> Result<ReviseSupplierOfferingResult> {
        write::revised(&self.db, prepared, executor).await
    }
    /// 核验对象和命令回执后，仅为允许手工维护且有责任归属的供给准备实时可供更新；不修改供给或商业修订。
    /// 旧版本、倒退来源时间或非法数量拒绝，缺投影返回 `NotFound`。
    /// 同键同载荷可返回原结果；异载荷冲突，损坏回执不能当作新命令执行。
    pub async fn prepare_availability(
        &self,
        id: &str,
        req: &UpdateSupplierOfferingAvailabilityRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CommandPreparation<PreparedAvailability, UpdateSupplierOfferingAvailabilityResult>> {
        req.validate()?;
        let fingerprint = req.command_fingerprint(id)?;
        let offering = match recovery::availability(self, id, req, actor, executor).await? {
            recovery::Replay::Stored(result) => return Ok(CommandPreparation::Replay(result)),
            recovery::Replay::Absent(offering) => {
                offering.ok_or_else(|| Error::Internal("可供更新缺少已授权供给".into()))?
            },
        };

        offering.ensure_manual_availability_writable()?;
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
    /// 用调用方事务执行器 CAS 更新可供投影，成功后写入含状态、版本及来源时间的原结果回执；任一步失败即返
    /// 回错误。
    pub async fn persist_availability(
        &self,
        prepared: &mut PreparedAvailability,
        executor: &mut dyn Executor,
    ) -> Result<UpdateSupplierOfferingAvailabilityResult> {
        write::availability(&self.db, prepared, executor).await
    }
    // 同一供应商 SKU 已有供给时拒绝创建。
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
    // 来源连接若存在，必须属于该供应商且为启用态；未绑定连接则跳过。
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
            .ok_or_else(|| Error::NotFound("供应商 API 连接不存在，请重新选择有效连接".to_string()))?;
        connection
            .ensure_offering_source(offering.supplier_id.as_ref())
            .map_err(|error| Error::BusinessLogicError(error.to_string()))
    }
    // 新增时指定的配送模板必须存在且属于本供应商；未指定时跟随供应商默认模板。
    async fn assign_requested_template(
        &self,
        offering: &mut SupplierOffering,
        template_id: Option<&str>,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let Some(template_id) = template_id.map(str::trim).filter(|id| !id.is_empty()) else {
            return Ok(());
        };
        let template = self
            .db
            .supplier_delivery_templates()
            .find_by_id(template_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("配送模板不存在".to_string()))?;
        if !template.belongs_to(&offering.supplier_id) {
            return Err(Error::ValidationError("配送模板不属于所选供应商".to_string()));
        }
        offering.assign_delivery_template(Some(SupplierDeliveryTemplateId::new(template.base.id)), actor);
        Ok(())
    }
    async fn ensure_api_prices(
        &self,
        offering: &SupplierOffering,
        revision: &mut SupplierOfferingRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if offering.source_type != OfferingSourceType::Api {
            return Ok(());
        }
        let id = offering
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::BusinessLogicError("API 供给当前条款缺失".into()))?;
        let current = self
            .db
            .supplier_offering_revisions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("API 供给当前条款不存在".into()))?;
        revision.source_purchase_terms = current.source_purchase_terms.clone();
        revision.ensure_api_prices_unchanged(&current)?;
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
    /// 在调用方的新快照内恢复创建命令的已提交结果。
    /// 当前资格及原供给授权通过时返回原回执；没有回执时返回 None。
    /// 当前身份、范围、回执指纹或读取失效时拒绝。
    /// request、actor 为原请求和当前身份，executor 为新的快照事务。
    pub async fn created_result(
        &self,
        request: &CreateSupplierOfferingRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<CreateSupplierOfferingResult>> {
        request.validate()?;
        Ok(match recovery::created(self, request, actor, executor).await? {
            recovery::Replay::Stored(result) => Some(result),
            recovery::Replay::Absent(_) => None,
        })
    }

    /// 在调用方的新快照内恢复修订命令的已提交结果。
    /// 当前修订授权通过时返回原回执；没有回执时返回 None。
    /// 当前身份、对象、回执指纹或读取失效时拒绝。
    /// id、request 为原目标与请求，actor 为当前身份，executor 为新的快照事务。
    pub async fn revised_result(
        &self,
        id: &str,
        request: &ReviseSupplierOfferingRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReviseSupplierOfferingResult>> {
        request.validate()?;
        Ok(match recovery::revised(self, id, request, actor, executor).await? {
            recovery::Replay::Stored(result) => Some(result),
            recovery::Replay::Absent(_) => None,
        })
    }

    /// 在调用方的新快照内恢复可供更新命令的已提交结果。
    /// 完整可供更新授权通过时返回原回执；没有回执时返回 None。
    /// 当前身份、对象、回执指纹或读取失效时拒绝。
    /// id、request 为原目标与请求，actor 为当前身份，executor 为新的快照事务。
    pub async fn availability_result(
        &self,
        id: &str,
        request: &UpdateSupplierOfferingAvailabilityRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<UpdateSupplierOfferingAvailabilityResult>> {
        request.validate()?;
        Ok(match recovery::availability(self, id, request, actor, executor).await? {
            recovery::Replay::Stored(result) => Some(result),
            recovery::Replay::Absent(_) => None,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::supplier_offering::{
        AvailabilityStatus, OfferingSourceType, SupplierOfferingCommandData,
    };
    // 验证历史裸指纹命令与 DTO 新指纹同键同载荷可重放，跨操作必须冲突。
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
                bulk_supply_price_gross: Some("9.00".to_string()),
                input_tax_rate: "0.13".to_string(),
                bulk_minimum_order_quantity: Some("10".to_string()),
                product_capabilities: vec!["DROP_SHIP".to_string()],
                valid_from: "2026-01-01".to_string(),
                valid_to: None,
            },
            delivery_template_id: None,
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
        assert_eq!(revision.bulk_minimum_order_quantity.unwrap().to_string(), "10");
        assert_eq!(revision.valid_from.to_string(), "2026-01-01");
    }
}
