//! 新供给身份与来源条款准备，保持原验证顺序并复用调用方事务。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId};
use id_generator::next_id;
use persistence_core::Executor;

use super::SupplierOfferingService;
use crate::Result;
use crate::dto::supplier_offering::{self as dto, CreateSupplierOfferingRequest};
use crate::entity::supplier_offering::{
    SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};

impl SupplierOfferingService {
    /// 在原供应资格核验完成后分配首份可供观察身份和接收时间。
    /// # 参数
    /// `request` 为原来源观察，`offering` 为首版所属供给，`actor` 为操作人。
    /// # 返回
    /// 与原创建流程同口径的初始可供观察。
    /// # 错误
    /// 数量、状态或来源观察字段无效时拒绝。
    pub(super) fn prepare_initial_availability(
        request: &CreateSupplierOfferingRequest,
        offering: SupplierOfferingId,
        actor: &AuditActor,
    ) -> Result<SupplierOfferingAvailability> {
        let received_at = Instant::now();
        let source_updated_at = dto::resolve_source_updated_at(request.source_updated_at, received_at);
        let data = request.try_into_availability_data(
            offering,
            source_updated_at,
            received_at,
            actor.id().to_string(),
        )?;
        Ok(SupplierOfferingAvailability::new(SupplierOfferingAvailabilityId::new(next_id()), data)?)
    }

    /// 按原顺序核验责任、供给身份、连接和配送模板。
    /// # 参数
    /// `request`、`actor` 为原创建入口输入，`executor` 沿用创建事务。
    /// # 返回
    /// 尚未写入的完整稳定供给。
    /// # 错误
    /// 责任、范围、身份、连接或配送模板核验失败时立即停止。
    pub(super) async fn prepare_initial_offering(
        &self,
        request: &CreateSupplierOfferingRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOffering> {
        let (maintainer, org) = self.access().maintainer_org(None, actor, executor).await?;
        let mut offering = SupplierOffering::new(
            SupplierOfferingId::new(next_id()),
            request.try_into_offering_data(maintainer, org)?,
            actor.id(),
        )?;
        self.access().require_create(actor, &offering, executor).await?;
        self.ensure_identity_available(&offering, executor).await?;
        self.ensure_source_connection(&offering, executor).await?;
        self.assign_requested_template(
            &mut offering,
            request.delivery_template_id.as_deref(),
            actor.id(),
            executor,
        )
        .await?;
        Ok(offering)
    }

    /// 构造来源只读采购约束已经绑定的首版修订。
    /// # 参数
    /// `request` 为原条款，`offering` 为身份及连接已核验的供给，执行器沿用创建事务。
    /// # 返回
    /// 在供应资格查询之前完成来源约束和采购价格核验的修订。
    /// # 错误
    /// 原条款无效、来源缺失、采购价或已报告税率不符时拒绝。
    pub(super) async fn prepare_initial_revision(
        &self,
        request: &CreateSupplierOfferingRequest,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingRevision> {
        let data = request.try_into_initial_revision(SupplierOfferingId::new(offering.base.id.clone()))?;
        let mut revision = SupplierOfferingRevision::new(SupplierOfferingRevisionId::new(next_id()), data)?;
        self.bind_api_purchase_terms(offering, &mut revision, executor).await?;
        Ok(revision)
    }
}
