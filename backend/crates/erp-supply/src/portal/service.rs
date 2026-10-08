//! 单域门户步骤；绑定有效性由身份组合层在同一事务重验。
use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SupplierAccountId, SupplierOfferingId, SupplierOfferingRevisionId};
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;
use serde_json::Value;
use validator::Validate;

use super::application::{ensure_supplier, ensure_text};
use super::{
    OfferingApplication, OfferingApplicationSnapshot, PortalAvailabilityFact, PortalAvailabilityInput,
    PortalAvailabilityUpdateResult, PortalCommandReceipt, PortalSupplyExt, validate_portal_reported_at,
};
use crate::dto::supplier_offering::{
    SupplierOfferingTermsWrite, UpdateSupplierOfferingAvailabilityRequest,
    UpdateSupplierOfferingAvailabilityResult,
};
use crate::entity::supplier_offering::write_data::parse_optional_quantity;
use crate::entity::supplier_offering::{
    OfferingSourceType, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};
use crate::repository::SupplierOfferingExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 新品领域消费的窄条款校验合同；禁止复制金额税率规则。
#[async_trait]
pub trait PortalTermsValidationPort: Send + Sync {
    /// 验证供给条款允许列表。
    /// # 参数
    /// `terms` 是供应商输入；`on_date` 是提交时业务日。
    /// # 返回
    /// 校验通过。
    /// # 错误
    /// 金额、税率、起订量或当前有效期非法时拒绝。
    async fn validate_terms(&self, terms: &SupplierOfferingTermsWrite, on_date: BusinessDate) -> Result<()>;
}
/// 复用正式供给实体验证门户条款。
/// # 参数
/// 条款及业务日。
/// # 返回
/// 类型化条款合法时成功。
/// # 错误
/// 非法金额税率、未来生效或已过期时拒绝。
pub fn validate_portal_terms(terms: &SupplierOfferingTermsWrite, on_date: BusinessDate) -> Result<()> {
    terms.validate()?;
    let data = terms.try_into_revision_data(SupplierOfferingId::new("validation-only"), 1)?;
    let revision = SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("validation-only"), data)?;
    if revision.valid_from > on_date || revision.valid_to.is_some_and(|to| to < on_date) {
        return Err(Error::BusinessLogicError("申请条款尚未生效或已经过期".into()));
    }
    Ok(())
}
/// 独立供应商授权路径，不伪装内部人员或授予公司DataScope。
pub struct PortalOfferingService {
    pub(super) db: Database,
}
impl PortalOfferingService {
    /// 创建本域门户步骤服务。
    /// # 参数
    /// `db` 为目标数据库。
    /// # 返回
    /// 无授权缓存的服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 加载申请供组合层执行归属或任务授权。
    /// # 参数
    /// 服务器申请标识及调用方执行器。
    /// # 返回
    /// 当前未删除申请。
    /// # 错误
    /// 不存在统一返回NotFound。
    pub async fn load_application(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<OfferingApplication> {
        self.db
            .portal_applications()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("申请不存在或无权查看".into()))
    }
    /// 在调用方事务保存申请状态；更新采用CAS。
    /// # 参数
    /// 已完成授权和纯规则迁移的申请，是否新建及执行器。
    /// # 返回
    /// 写入成功并推进持久化版本。
    /// # 错误
    /// 唯一性、版本或数据库错误。
    pub async fn save_application(
        &self,
        app: &mut OfferingApplication,
        is_new: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if is_new {
            self.db.portal_applications().create(app, executor).await?;
        } else {
            let before = self.load_application(&app.base.id, executor).await?;
            app.ensure_history_preserved(&before)?;
            self.db.portal_applications().update(app, executor).await?;
        }
        Ok(())
    }
    /// 加载本供应商的供给，未知及范围外使用相同结果。
    /// # 参数
    /// 当前有效绑定、真实供应商身份和供给标识。
    /// # 返回
    /// 本供应商供给。
    /// # 错误
    /// 外部身份无效或供给不可见时拒绝。
    pub async fn require_owned_offering(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOffering> {
        ensure_supplier(actor, supplier_id)?;
        self.db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .filter(|offering| offering.supplier_id.as_ref() == supplier_id)
            .ok_or_else(|| Error::NotFound("供给不存在或无权查看".into()))
    }
    /// 拒绝门户覆盖API管理事实。
    /// # 参数
    /// 当前已授权供给。
    /// # 返回
    /// 手工或Excel来源成功。
    /// # 错误
    /// API来源只读。
    pub fn ensure_portal_writable(offering: &SupplierOffering) -> Result<()> {
        if offering.source_type == OfferingSourceType::Api {
            return Err(Error::Forbidden("API自动同步供给在门户中只读".into()));
        }
        Ok(())
    }
    /// 在确认事务重验首次报价开放资格。
    /// # 参数
    /// 服务器供应商绑定和精确SKU标识。
    /// # 返回
    /// 定向开放仍有效时成功。
    /// # 错误
    /// 未开放或撤销统一拒绝。
    pub async fn ensure_quote_access(
        &self,
        supplier_id: &str,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let grant = self
            .db
            .portal_quote_grants()
            .find_one(doc! {"supplier_id": supplier_id, "sku_id": sku_id, "active": true}, executor)
            .await?;
        if grant.is_none() {
            return Err(Error::NotFound("SKU不存在或无报价资格".into()));
        }
        Ok(())
    }
    /// 更新本供应商可供事实；不修改条款、来源或关系状态。
    /// # 参数
    /// 绑定、真实供应商操作人、目标、允许列表及执行器。
    /// # 返回
    /// 当前正式可供状态和CAS新版本。
    /// # 错误
    /// API来源、范围外、版本或输入无效时拒绝。
    pub async fn update_availability(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        id: &str,
        req: &PortalAvailabilityInput,
        executor: &mut dyn Executor,
    ) -> Result<UpdateSupplierOfferingAvailabilityResult> {
        Ok(self.update_availability_with_change(supplier_id, actor, id, req, executor).await?.current)
    }
    /// 更新可供并返回同事务读取的变更前后事实。
    /// # 参数
    /// 当前有效供应商绑定、真实供应商身份、目标和允许列表。
    /// # 返回
    /// 原成功结果以及审计before/after。
    /// # 错误
    /// 归属、API来源、版本或输入非法时拒绝。
    pub async fn update_availability_with_change(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        id: &str,
        req: &PortalAvailabilityInput,
        executor: &mut dyn Executor,
    ) -> Result<PortalAvailabilityUpdateResult> {
        if executor.session().is_none() {
            return Err(Error::Internal("门户可供命令必须使用调用方事务".into()));
        }
        let (before, mut availability) =
            self.prepare_availability(supplier_id, actor, id, req, executor).await?;
        self.db.supplier_offering_availabilities().update(&mut availability, executor).await?;
        let after = PortalAvailabilityFact::from_availability(&availability);
        Ok(PortalAvailabilityUpdateResult {
            before,
            after,
            current: UpdateSupplierOfferingAvailabilityResult {
                offering_id: id.to_string(),
                availability_status: availability.availability_status,
                availability_version: availability.base.version,
                source_updated_at: availability.source_updated_at.unix_secs(),
                safety_pause: None,
            },
        })
    }
    /// 批量整批预检复用同一归属、API只读、版本和数量规则，不写入。
    /// # 参数
    /// 当前供应商绑定、操作人、目标、版本输入和执行器。
    /// # 返回
    /// 当前事实仍可更新时成功。
    /// # 错误
    /// 身份、来源、版本、数量或时间冲突拒绝。
    pub async fn validate_availability(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        id: &str,
        req: &PortalAvailabilityInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.prepare_availability(supplier_id, actor, id, req, executor).await?;
        Ok(())
    }
    /// 在内存中套用可供变更并返回前后事实，此处不写库。
    async fn prepare_availability(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        id: &str,
        req: &PortalAvailabilityInput,
        executor: &mut dyn Executor,
    ) -> Result<(PortalAvailabilityFact, SupplierOfferingAvailability)> {
        ensure_text(&req.reason, "变更原因")?;
        ensure_text(&req.idempotency_key, "操作号")?;
        let offering = self.require_owned_offering(supplier_id, actor, id, executor).await?;
        Self::ensure_portal_writable(&offering)?;
        let offering_id = SupplierOfferingId::new(id);
        let mut availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&offering_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("可供状态不存在".into()))?;
        availability
            .ensure_version(req.expected_version)
            .map_err(|_| Error::ConflictError("可供状态已变化，请保留输入并重新核对".into()))?;
        let before = PortalAvailabilityFact::from_availability(&availability);
        let now = Instant::now();
        let data = UpdateSupplierOfferingAvailabilityRequest {
            expected_version: Some(req.expected_version),
            availability_status: req.availability_status.into(),
            available_quantity: req.available_quantity.clone(),
            source_updated_at: Some(now.unix_secs()),
            source_revision_token: None,
            change_reason: req.reason.clone(),
            idempotency_key: req.idempotency_key.clone(),
        }
        .try_into_availability_data(offering_id, now, now, actor.id().to_string())?;
        availability.apply(data)?;
        Ok((before, availability))
    }
    /// 恢复独立成功回执，读取包括软删除记录以失败关闭。
    /// # 参数
    /// 原规范命令和执行器。
    /// # 返回
    /// 已成功命令返回原结果；不存在返回None。
    /// # 错误
    /// 载荷不符或回执损坏拒绝。
    pub async fn command_result(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<Value>> {
        self.db
            .portal_command_receipts()
            .find_one_by_field_including_deleted("id", command.id(), executor)
            .await?
            .map(|receipt| receipt.replay(command))
            .transpose()
    }
    /// 与正式结果及审计同事务登记原命令结果。
    /// # 参数
    /// 原命令、供应商归属、原结果及执行器。
    /// # 返回
    /// 回执成功写入。
    /// # 错误
    /// 身份无效、重复命令或数据库失败。
    pub async fn command_commit(
        &self,
        command: &CommandReceipt,
        supplier_id: &str,
        result: &Value,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt = PortalCommandReceipt::new(command, supplier_id, result)?;
        if executor.session().is_none() {
            return Err(Error::Internal("门户命令回执必须与业务事实共用事务".into()));
        }
        self.db.portal_command_receipts().create(&receipt, executor).await?;
        Ok(())
    }
    /// 检查供应商订货编码身份，禁止同编码改绑SKU。
    /// # 参数
    /// 服务端供应商、精确SKU与订货编码。
    /// # 返回
    /// 尚不存在或相同SKU的已有供给。
    /// # 错误
    /// 编码已关联其他SKU时冲突。
    pub async fn ordering_identity(
        &self,
        supplier_id: &str,
        sku_id: &str,
        code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOffering>> {
        let offering = self
            .db
            .supplier_offerings()
            .find_by_supplier_identity(&SupplierAccountId::new(supplier_id), code.trim(), executor)
            .await?;
        if offering.as_ref().is_some_and(|item| item.sku_id.as_ref() != sku_id) {
            return Err(Error::ConflictError("供应商订货编码已关联其他SKU".into()));
        }
        Ok(offering)
    }
    /// 提交前校验原稿身份，首次报价存在相同供给时转为冻结版本修订。
    /// # 参数
    /// 当前有效绑定、供应商操作人、原稿、业务日及执行器。
    /// # 返回
    /// 身份和正式基线已核对的快照。
    /// # 错误
    /// 开放失效、跨供应商、API来源、版本或条款非法拒绝。
    pub async fn prepare_snapshot(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        snapshot: OfferingApplicationSnapshot,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<OfferingApplicationSnapshot> {
        ensure_supplier(actor, supplier_id)?;
        match snapshot {
            quote @ OfferingApplicationSnapshot::ExistingQuote { .. } => {
                self.prepare_quote_snapshot(supplier_id, quote, on_date, executor).await
            },
            target => {
                let (id, version, revision_no) =
                    target.target().ok_or_else(|| Error::ValidationError("申请目标无效".into()))?;
                let offering = self.require_owned_offering(supplier_id, actor, id, executor).await?;
                Self::ensure_portal_writable(&offering)?;
                if offering.base.version != version
                    || self.current_revision(&offering, executor).await?.revision.revision_no != revision_no
                {
                    return Err(Error::ConflictError("供给版本已经变化，请重新核对".into()));
                }
                if let OfferingApplicationSnapshot::TermsChange { terms, .. } = &target {
                    validate_portal_terms(terms, on_date)?;
                }
                Ok(target)
            },
        }
    }
    /// 订货编码已占用时改成条款修订快照，否则重验报价资格与可供填报。
    async fn prepare_quote_snapshot(
        &self,
        supplier_id: &str,
        quote: OfferingApplicationSnapshot,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<OfferingApplicationSnapshot> {
        let OfferingApplicationSnapshot::ExistingQuote {
            sku_id,
            target_version,
            supplier_sku_code,
            supplier_product_code,
            terms,
            availability_status,
            available_quantity,
            availability_reported_at,
        } = quote
        else {
            return Err(Error::ValidationError("首次报价快照无效".into()));
        };
        ensure_text(&sku_id, "公司SKU")?;
        ensure_text(&supplier_sku_code, "供应商订货编码")?;
        validate_portal_terms(&terms, on_date)?;
        if let Some(offering) =
            self.ordering_identity(supplier_id, &sku_id, &supplier_sku_code, executor).await?
        {
            Self::ensure_portal_writable(&offering)?;
            let revision = self.current_revision(&offering, executor).await?;
            return Ok(OfferingApplicationSnapshot::TermsChange {
                offering_id: offering.base.id,
                expected_offering_version: offering.base.version,
                expected_revision_no: revision.revision.revision_no,
                terms,
            });
        }
        self.ensure_quote_access(supplier_id, &sku_id, executor).await?;
        parse_optional_quantity(available_quantity.as_deref())?;
        validate_portal_reported_at(availability_reported_at, Instant::now())?;
        Ok(OfferingApplicationSnapshot::ExistingQuote {
            sku_id,
            target_version,
            supplier_sku_code,
            supplier_product_code,
            terms,
            availability_status,
            available_quantity,
            availability_reported_at,
        })
    }
}
#[async_trait]
impl PortalTermsValidationPort for PortalOfferingService {
    async fn validate_terms(&self, terms: &SupplierOfferingTermsWrite, on_date: BusinessDate) -> Result<()> {
        validate_portal_terms(terms, on_date)
    }
}
