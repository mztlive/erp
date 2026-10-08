//! 在审核用例的Executor中建立或修订正式供给；无内部身份伪装。
use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SkuId, SupplierAccountId, SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId,
};
use id_generator::next_id;
use persistence_core::Executor;

use super::application::ensure_text;
use super::{
    ApplicationStatus, OfferingApplication, OfferingApplicationResult, OfferingApplicationSnapshot,
    PortalAvailabilityStatus, PortalOfferingService, validate_portal_reported_at, validate_portal_terms,
};
use crate::dto::supplier_offering::SupplierOfferingTermsWrite;
use crate::entity::supplier_offering::write_data::parse_optional_quantity;
use crate::entity::supplier_offering::{
    OfferingRevisionImpact, OfferingSourceType, OfferingStatus, SupplierOffering,
    SupplierOfferingAvailability, SupplierOfferingAvailabilityData, SupplierOfferingData,
    SupplierOfferingRevision,
};
use crate::ports::offering_qualification::{PortalQuoteQualificationPort, QualificationPort};
use crate::repository::SupplierOfferingExt;
use crate::{Error, Result};

/// 内部审核授权完成后提供的正式落地上下文。
pub struct ConfirmedOfferingInput<'a> {
    pub application: &'a OfferingApplication,
    pub actor: &'a AuditActor,
    pub maintainer_user_id: &'a str,
    pub business_org_unit_id: &'a str,
    pub on_date: BusinessDate,
}
/// 正式供给变更结果，仍需同事务完成申请、任务与审计。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfirmedOfferingResult {
    pub result: OfferingApplicationResult,
    pub created: bool,
}
struct RevisionInput<'a> {
    context: &'a ConfirmedOfferingInput<'a>,
    id: &'a str,
    expected_version: u64,
    expected_revision_no: u32,
    terms: Option<&'a SupplierOfferingTermsWrite>,
}
struct CreateInput<'a> {
    supplier_id: &'a str,
    sku_id: &'a str,
    supplier_sku_code: &'a str,
    supplier_product_code: Option<String>,
    terms: &'a SupplierOfferingTermsWrite,
    availability_status: PortalAvailabilityStatus,
    available_quantity: Option<String>,
    availability_reported_at: Instant,
    context: &'a ConfirmedOfferingInput<'a>,
}
impl PortalOfferingService {
    /// 对提交快照建立或修订正式供给；所有读取写入使用审核Executor。
    /// # 参数
    /// 审核上下文、当前资格Port及执行器。
    /// # 返回
    /// 唯一正式结果及是否首次新增。
    /// # 错误
    /// 非法身份、版本、开放资格、API来源或条款失效时拒绝。
    pub async fn apply_application<P: PortalQuoteQualificationPort + ?Sized>(
        &self,
        input: ConfirmedOfferingInput<'_>,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<ConfirmedOfferingResult, P::Error> {
        ensure_confirmation(&input)?;
        ensure_transaction(executor)?;
        match &input.application.snapshot {
            OfferingApplicationSnapshot::ExistingQuote { sku_id, target_version, .. } => {
                self.ensure_quote_access(&input.application.supplier_id, sku_id, executor).await?;
                qualification.ensure_quote_target(&SkuId::new(sku_id), target_version, executor).await?;
                self.create_confirmed(CreateInput::from_context(&input)?, qualification, executor).await
            },
            OfferingApplicationSnapshot::TermsChange {
                offering_id,
                expected_offering_version,
                expected_revision_no,
                terms,
            } => {
                self.revise_confirmed(
                    RevisionInput {
                        context: &input,
                        id: offering_id,
                        expected_version: *expected_offering_version,
                        expected_revision_no: *expected_revision_no,
                        terms: Some(terms),
                    },
                    qualification,
                    executor,
                )
                .await
            },
            OfferingApplicationSnapshot::StopSupply {
                offering_id,
                expected_offering_version,
                expected_revision_no,
            } => {
                self.revise_confirmed(
                    RevisionInput {
                        context: &input,
                        id: offering_id,
                        expected_version: *expected_offering_version,
                        expected_revision_no: *expected_revision_no,
                        terms: None,
                    },
                    qualification,
                    executor,
                )
                .await
            },
        }
    }
    /// 新品审核复用的首版供给步骤；不要求新品先有定向开放记录。
    /// # 参数
    /// 新品SKU已由同事务正式建档，上下文保留原申请及内部责任。
    /// # 返回
    /// 供给三元组结果。
    /// # 错误
    /// 身份、资格、订货编码冲突或条款非法拒绝。
    pub async fn create_for_new_product<P: QualificationPort + ?Sized>(
        &self,
        input: ConfirmedOfferingInput<'_>,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<ConfirmedOfferingResult, P::Error> {
        ensure_confirmation(&input)?;
        ensure_transaction(executor)?;
        if let OfferingApplicationSnapshot::TermsChange {
            offering_id,
            expected_offering_version,
            expected_revision_no,
            terms,
        } = &input.application.snapshot
        {
            return self
                .revise_confirmed(
                    RevisionInput {
                        context: &input,
                        id: offering_id,
                        expected_version: *expected_offering_version,
                        expected_revision_no: *expected_revision_no,
                        terms: Some(terms),
                    },
                    qualification,
                    executor,
                )
                .await;
        }
        self.create_confirmed(CreateInput::from_context(&input)?, qualification, executor).await
    }
    /// 校验条款与资格后写入供给、首版修订和可供投影；订货身份已存在则冲突。
    async fn create_confirmed<P: QualificationPort + ?Sized>(
        &self,
        input: CreateInput<'_>,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<ConfirmedOfferingResult, P::Error> {
        validate_portal_terms(input.terms, input.context.on_date)?;
        if self
            .ordering_identity(input.supplier_id, input.sku_id, input.supplier_sku_code, executor)
            .await?
            .is_some()
        {
            return Err(Error::ConflictError("供给已经存在，请重新核对条款修订".into()).into());
        }
        qualification
            .ensure_qualified(
                &SupplierAccountId::new(input.supplier_id),
                &SkuId::new(input.sku_id),
                input.context.on_date,
                executor,
            )
            .await?;
        let offering_id = SupplierOfferingId::new(next_id());
        let mut offering = input.offering(offering_id.clone())?;
        let revision = SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new(next_id()),
            input.terms.try_into_revision_data(offering_id.clone(), 1)?,
        )?;
        let availability = input.availability(offering_id)?;
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        self.db
            .supplier_offering_repository()
            .create_with_revision_and_availability(&offering, &revision, &availability, executor)
            .await?;
        Ok(ConfirmedOfferingResult { result: formal_result(&offering, &revision, "CREATED"), created: true })
    }
    /// 有新条款时追加修订，无差异则复用当前修订；无条款时把供给标为停止。
    async fn revise_confirmed<P: QualificationPort + ?Sized>(
        &self,
        revision: RevisionInput<'_>,
        qualification: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<ConfirmedOfferingResult, P::Error> {
        let RevisionInput { context: input, id, expected_version, expected_revision_no, terms } = revision;
        let mut offering =
            self.confirmation_target(input, id, expected_version, expected_revision_no, executor).await?;
        let current = self.current_revision(&offering, executor).await?;
        if let Some(terms) = terms {
            validate_portal_terms(terms, input.on_date)?;
            qualification
                .ensure_qualified(&offering.supplier_id, &offering.sku_id, input.on_date, executor)
                .await?;
            let next = offering.next_revision_no(current.revision.revision_no, expected_revision_no)?;
            let revision = SupplierOfferingRevision::new(
                SupplierOfferingRevisionId::new(next_id()),
                terms.try_into_revision_data(SupplierOfferingId::new(id), next)?,
            )?;
            if revision.impact_from(&current) == OfferingRevisionImpact::None {
                return Ok(ConfirmedOfferingResult {
                    result: formal_result(&offering, &current, "REUSED"),
                    created: false,
                });
            }
            offering.stable.current_revision_id = Some(revision.base.id.clone());
            offering.stable.updated_by = input.actor.id().to_string();
            self.db
                .supplier_offering_repository()
                .append_revision(&mut offering, &revision, executor)
                .await?;
            Ok(ConfirmedOfferingResult {
                result: formal_result(&offering, &revision, "REVISED"),
                created: false,
            })
        } else {
            offering.update_status(OfferingStatus::Stopped, input.actor.id())?;
            self.db.supplier_offerings().update(&mut offering, executor).await?;
            Ok(ConfirmedOfferingResult {
                result: formal_result(&offering, &current, "STOPPED"),
                created: false,
            })
        }
    }
    /// 读取本供应商可写供给，并拒绝版本漂移、条款号变化或已停止供给。
    async fn confirmation_target(
        &self,
        input: &ConfirmedOfferingInput<'_>,
        id: &str,
        version: u64,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOffering> {
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .filter(|o| o.supplier_id.as_ref() == input.application.supplier_id)
            .ok_or_else(|| Error::NotFound("供给不存在或无权查看".into()))?;
        Self::ensure_portal_writable(&offering)?;
        if offering.base.version != version {
            return Err(Error::ConflictError("供给版本已经变化，请供应商重新核对并提交".into()));
        }
        if self.current_revision(&offering, executor).await?.revision.revision_no != revision_no {
            return Err(Error::ConflictError("供给条款版本已经变化".into()));
        }
        if offering.stable.status == OfferingStatus::Stopped {
            return Err(Error::BusinessLogicError("已停止供给不得通过再次报价恢复".into()));
        }
        Ok(offering)
    }
    /// 在当前执行器加载正式条款指针，缺失引用失败关闭。
    /// # 参数
    /// 当前已授权供给及执行器。
    /// # 返回
    /// 当前不可变正式条款。
    /// # 错误
    /// 引用缺失或数据库失败。
    pub(super) async fn current_revision(
        &self,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingRevision> {
        let id = offering
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::Internal("供给缺少当前条款".into()))?;
        let revision = self
            .db
            .supplier_offering_revisions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::Internal("供给当前条款不存在".into()))?;
        if revision.supplier_offering_id.as_ref() != offering.base.id {
            return Err(Error::Internal("供给当前条款引用不属于该供给".into()));
        }
        Ok(revision)
    }
}
/// 确认人须为内部账号，申请仍为已提交，且当前快照等于最后一次冻结提交。
fn ensure_confirmation(input: &ConfirmedOfferingInput<'_>) -> Result<()> {
    if input.actor.kind() != AccountKind::Admin {
        return Err(Error::Forbidden("仅内部人员可确认供给".into()));
    }
    if input.application.status != ApplicationStatus::Submitted {
        return Err(Error::ConflictError("申请已不再等待确认".into()));
    }
    let submitted =
        input.application.submissions.last().ok_or_else(|| Error::Internal("申请缺少冻结快照".into()))?;
    let current = serde_json::to_value(&input.application.snapshot)
        .map_err(|error| Error::Internal(error.to_string()))?;
    let frozen =
        serde_json::to_value(&submitted.snapshot).map_err(|error| Error::Internal(error.to_string()))?;
    if current != frozen {
        return Err(Error::ConflictError("供应商提交快照不得在审核时改写".into()));
    }
    ensure_text(input.maintainer_user_id, "供给内部维护人")?;
    ensure_text(input.business_org_unit_id, "供给业务组织")
}
/// 没有调用方事务会话时拒绝正式供给写入。
fn ensure_transaction(executor: &mut dyn Executor) -> Result<()> {
    if executor.session().is_none() {
        return Err(Error::Internal("门户正式供给写入必须使用调用方事务".into()));
    }
    Ok(())
}
fn formal_result(
    offering: &SupplierOffering,
    revision: &SupplierOfferingRevision,
    operation: &str,
) -> OfferingApplicationResult {
    OfferingApplicationResult {
        offering_id: offering.base.id.clone(),
        revision_id: revision.base.id.clone(),
        revision_no: revision.revision.revision_no,
        offering_version: offering.base.version,
        operation: operation.to_string(),
    }
}

impl<'a> CreateInput<'a> {
    /// 只从首次报价快照组装创建输入，其它申请类型返回校验错误。
    fn from_context(context: &'a ConfirmedOfferingInput<'a>) -> Result<Self> {
        let OfferingApplicationSnapshot::ExistingQuote {
            sku_id,
            supplier_sku_code,
            supplier_product_code,
            terms,
            availability_status,
            available_quantity,
            availability_reported_at,
            ..
        } = &context.application.snapshot
        else {
            return Err(Error::ValidationError("首版供给需要首次报价快照".into()));
        };
        Ok(Self {
            supplier_id: &context.application.supplier_id,
            sku_id,
            supplier_sku_code,
            supplier_product_code: supplier_product_code.clone(),
            terms,
            availability_status: *availability_status,
            available_quantity: available_quantity.clone(),
            availability_reported_at: *availability_reported_at,
            context,
        })
    }
    fn offering(&self, id: SupplierOfferingId) -> Result<SupplierOffering> {
        Ok(SupplierOffering::new(
            id,
            SupplierOfferingData {
                sku_id: SkuId::new(self.sku_id),
                supplier_id: SupplierAccountId::new(self.supplier_id),
                supplier_product_code: self.supplier_product_code.clone(),
                supplier_sku_code: self.supplier_sku_code.to_string(),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
                maintainer_user_id: self.context.maintainer_user_id.to_string(),
                business_org_unit_id: self.context.business_org_unit_id.to_string(),
            },
            self.context.actor.id(),
        )?)
    }
    /// 校验填报时间与数量后构造可供投影，更新人取最后一次提交人。
    fn availability(&self, id: SupplierOfferingId) -> Result<SupplierOfferingAvailability> {
        let received_at = Instant::now();
        validate_portal_reported_at(self.availability_reported_at, received_at)?;
        let quantity = parse_optional_quantity(self.available_quantity.as_deref())?;
        let submitted_by = self
            .context
            .application
            .submissions
            .last()
            .map(|s| s.submitted_by.clone())
            .ok_or_else(|| Error::Internal("申请缺少提交人".into()))?;
        Ok(SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new(next_id()),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: id,
                availability_status: self.availability_status.into(),
                available_quantity: quantity,
                source_updated_at: self.availability_reported_at,
                received_at,
                source_revision_token: None,
                updated_by: submitted_by,
            },
        )?)
    }
}
