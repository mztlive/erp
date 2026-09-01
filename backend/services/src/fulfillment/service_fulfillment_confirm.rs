//! 线下服务履约确认：写入现场事实与图片凭证后由草稿迁到已确认。

use std::collections::HashSet;

use database::{AccessControlExt, FileAssetExt, FulfillmentExt, PurchaseOrderExt, Transactional};
use entities::common::time::Instant;
use entities::fulfillment::{
    ActualServiceLocation, ServiceEvidencePolicy, ServiceFulfillment, ServiceFulfillmentConfirmation,
};
use entities::ids::{FileAssetId, ServiceFulfillmentId};
use mongodb::{ClientSession, Database};
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::file_asset::PendingFileAssetRequest;
use crate::party::SensitiveDataCodec;
use crate::pending_file_assets::PendingFileAssets;

use super::purchase_context::{ensure_allocation_valid, ensure_po_fulfillable, ensure_prepay_gate};
use super::{ConfirmServiceFulfillmentRequest, FulfillmentService, ServiceFulfillmentView};

impl FulfillmentService {
    /// 确认服务履约（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 不携带新上传对象的内部兼容入口；HTTP 确认使用
    /// [`Self::confirm_service_fulfillment_with_assets`]。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `req` - 现场事实、乐观锁版本与图片凭证
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// 校验失败、状态冲突、门槛未满足或事务提交结果未知时返回错误。
    pub async fn confirm_service_fulfillment(
        &self,
        id: &str,
        req: ConfirmServiceFulfillmentRequest,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        self.confirm_service_fulfillment_with_assets(id, req, Vec::new(), actor)
            .await
    }

    /// 确认服务履约，同时登记本次上传的现场图片凭证。
    ///
    /// 门槛、采购销售分配、凭证资产、履约记录、任务完成和审计位于同一事务。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `req` - 现场事实、乐观锁版本与图片凭证
    /// * `asset_requests` - 本次 multipart 待登记文件
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// * `ValidationError` - 地点、时间、说明、数量或图片不合法
    /// * `NotFound` - 记录/采购单/分配/凭证不存在
    /// * `ConflictError` - 状态不允许确认或版本已变化
    /// * `BusinessLogicError` - 门槛未满足或分配无效
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.service_fulfillment_confirm",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "service_fulfillment_confirm"
        )
    )]
    pub async fn confirm_service_fulfillment_with_assets(
        &self,
        id: &str,
        mut req: ConfirmServiceFulfillmentRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        req.validate()?;
        validate_service_evidence_pending_requests(&asset_requests)?;
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let evidence_attachment_id = resolve_service_evidence_id(&mut req, &pending_assets)?;
        let confirmation = service_confirmation_from_request(
            &req,
            evidence_attachment_id,
            &self.fingerprint_key,
            self.sensitive_data.as_ref(),
        )?;
        persist_confirmed_service_fulfillment(
            &self.db,
            ServiceFulfillmentId::new(id.to_string()),
            req.version,
            confirmation,
            pending_assets,
            actor.clone(),
        )
        .await
    }
}

/// 把确认命令写成已规范化的现场事实。
///
/// 地点占位值与空白规则由领域值对象 [`ActualServiceLocation`] 独占；本函数
/// 只负责把已校验明文交给 Service 的加密编解码器与查询指纹函数。
///
/// # 参数
/// * `req` - 已通过校验的确认命令
/// * `evidence_attachment_id` - 已解析的正式凭证主键
/// * `fingerprint_key` - 服务地点查询指纹密钥
/// * `sensitive_data` - 服务地点加密编解码器
///
/// # 返回
/// 返回可写入草稿的确认事实。
///
/// # 错误
/// 实体规范化失败时返回校验错误。
fn service_confirmation_from_request(
    req: &ConfirmServiceFulfillmentRequest,
    evidence_attachment_id: FileAssetId,
    fingerprint_key: &[u8],
    sensitive_data: &SensitiveDataCodec,
) -> Result<ServiceFulfillmentConfirmation> {
    let service_location = ActualServiceLocation::parse(&req.service_location)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(ServiceFulfillmentConfirmation::new(
        req.result,
        req.completion_note.clone(),
        evidence_attachment_id,
        sensitive_data.encrypt(service_location.as_str())?,
        ServiceFulfillment::service_location_fingerprint(service_location.as_str(), fingerprint_key),
        Instant::from_unix_secs(req.service_started_at),
        Instant::from_unix_secs(req.service_ended_at),
        req.quantity,
    )?)
}

/// 在事务内写入现场事实、凭证资产并确认服务履约。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_id` - 服务履约主键
/// * `expected_version` - 调用方看到的乐观锁版本
/// * `confirmation` - 已规范化的确认现场事实
/// * `pending_assets` - 本次待登记图片凭证
/// * `actor` - 已通过鉴权的审计操作人
///
/// # 返回
/// 返回确认后的记录视图。
///
/// # 错误
/// 记录不存在、版本冲突、门槛失败或事务提交失败时返回错误。
async fn persist_confirmed_service_fulfillment(
    db: &Database,
    record_id: ServiceFulfillmentId,
    expected_version: u64,
    confirmation: ServiceFulfillmentConfirmation,
    pending_assets: PendingFileAssets,
    actor: AuditActor,
) -> Result<ServiceFulfillmentView> {
    let db = db.clone();
    let client = db.client().clone();
    let confirmed = client
        .with_transaction(move |session| {
            Box::pin(async move {
                confirm_service_fulfillment_in_transaction(
                    &db,
                    &record_id,
                    expected_version,
                    confirmation,
                    &pending_assets,
                    &actor,
                    session,
                )
                .await
            })
        })
        .await?;
    Ok(confirmed.into())
}

/// 在调用方事务内完成门槛校验、凭证登记与状态迁移。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_id` - 服务履约主键
/// * `expected_version` - 调用方看到的乐观锁版本
/// * `confirmation` - 已规范化的确认现场事实
/// * `pending_assets` - 本次待登记图片凭证
/// * `actor` - 已通过鉴权的审计操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回确认后的实体。
///
/// # 错误
/// 记录不存在、版本冲突、门槛失败或凭证不合法时返回错误。
async fn confirm_service_fulfillment_in_transaction(
    db: &Database,
    record_id: &ServiceFulfillmentId,
    expected_version: u64,
    confirmation: ServiceFulfillmentConfirmation,
    pending_assets: &PendingFileAssets,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<ServiceFulfillment> {
    let mut record = db
        .service_fulfillments()
        .find_by_id(record_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("服务履约记录不存在".to_string()))?;
    record
        .ensure_draft_version(expected_version)
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let po = db
        .purchase_orders()
        .find_by_id(record.purchase_order_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
    ensure_po_fulfillable(&po)?;
    ensure_prepay_gate(db, session, &po).await?;
    ensure_allocation_valid(
        db,
        session,
        &po,
        &record.purchase_line_sales_allocation_id,
        &record.sales_order_line_id,
    )
    .await?;
    ensure_service_evidence_asset_in_transaction(
        db,
        &confirmation.evidence_attachment_id,
        pending_assets,
        session,
    )
    .await?;
    pending_assets.persist(db, session).await?;
    record.apply_confirmation(confirmation)?;
    record.confirm()?;
    db.service_fulfillments().update(&mut record, session).await?;
    super::task::complete_fulfillment_task(
        db,
        super::task::FulfillmentTaskObject::ServiceFulfillment(&record),
        actor.id(),
        session,
    )
    .await?;
    if record.is_acceptance_eligible() {
        super::customer_acceptance_task::ensure_customer_acceptance_task(
            db,
            &po.sales_order_id,
            super::customer_acceptance_task::CustomerAcceptanceTaskReason::DeliveryAvailable,
            session,
        )
        .await?;
    }
    let audit = actor.clone().resource_log(
        "service_fulfillment.confirm",
        "service_fulfillment",
        record_id.to_string(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(record)
}

/// 校验本次待登记图片凭证的类型、敏感级别和保留策略。
///
/// 元数据规则由领域 [`ServiceEvidencePolicy`] 独占；待登记与既有资产两条
/// 路径必须使用同一策略，本函数只是按批次逐项应用。
///
/// # 参数
/// * `requests` - 待登记文件资产
///
/// # 返回
/// 全部合法或列表为空时返回 `Ok(())`。
///
/// # 错误
/// 图片类型、敏感级别或保留策略不满足时返回校验错误。
fn validate_service_evidence_pending_requests(requests: &[PendingFileAssetRequest]) -> Result<()> {
    for request in requests {
        ServiceEvidencePolicy::validate(
            &request.registration.content_type,
            request.registration.sensitivity_class,
            request.registration.retention_class,
            false,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    }
    Ok(())
}

/// 把确认命令中的临时凭证引用替换为本批次正式资产 ID。
///
/// # 参数
/// * `req` - 确认命令
/// * `pending_assets` - 本次待登记图片凭证
///
/// # 返回
/// 返回正式凭证主键。
///
/// # 错误
/// 引用了未上传文件或存在未被引用的上传文件时返回校验错误。
fn resolve_service_evidence_id(
    req: &mut ConfirmServiceFulfillmentRequest,
    pending_assets: &PendingFileAssets,
) -> Result<FileAssetId> {
    let mut used = HashSet::new();
    pending_assets.resolve_id(&mut req.evidence_attachment_id, &mut used)?;
    pending_assets.ensure_all_used(&used)?;
    Ok(req.evidence_attachment_id.clone())
}

/// 在确认事务内校验正式或本批次待登记的现场图片凭证。
///
/// 本批次待登记资产在事务前已按 [`ServiceEvidencePolicy`] 完成元数据校验，
/// 此处只识别其属于本批次；正式资产按同一策略校验持久化元数据。
///
/// # 参数
/// * `db` - 数据库实例
/// * `asset_id` - 正式凭证主键
/// * `pending_assets` - 本次待登记图片凭证
/// * `session` - 事务会话
///
/// # 返回
/// 凭证可用时返回 `Ok(())`。
///
/// # 错误
/// 凭证不存在、已销毁或元数据不合法时返回错误。
async fn ensure_service_evidence_asset_in_transaction(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &PendingFileAssets,
    session: &mut ClientSession,
) -> Result<()> {
    if pending_assets.contains_id(asset_id) {
        return Ok(());
    }
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("现场图片凭证不存在".to_string()))?;
    ServiceEvidencePolicy::validate(
        &asset.content_type,
        asset.sensitivity_class,
        asset.retention_class,
        asset.destroyed_at.is_some(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::service_confirmation_from_request;
    use crate::party::SensitiveDataCodec;
    use entities::fulfillment::{FulfillmentResult, ServiceFulfillment};
    use entities::ids::FileAssetId;
    use entities::money::Quantity;
    use std::str::FromStr;

    use super::ConfirmServiceFulfillmentRequest;

    /// 确认边界只持久化密文，并以同一份规范化明文计算查询指纹。
    #[test]
    fn confirmation_encrypts_normalized_service_location() {
        let sensitive_data = SensitiveDataCodec::from_secret(b"test-secret-that-is-at-least-32-bytes");
        let fingerprint_key = b"test-service-location-fingerprint";
        let request = ConfirmServiceFulfillmentRequest {
            version: 1,
            result: FulfillmentResult::Success,
            completion_note: "上门安装完成".to_string(),
            service_location: "  客户现场  ".to_string(),
            service_started_at: 1_700_000_000,
            service_ended_at: 1_700_003_600,
            quantity: Quantity::from_str("1").unwrap(),
            evidence_attachment_id: FileAssetId::new("file-1"),
        };

        let confirmation = service_confirmation_from_request(
            &request,
            FileAssetId::new("file-1"),
            fingerprint_key,
            &sensitive_data,
        )
        .unwrap();

        assert_ne!(confirmation.service_location_encrypted, "客户现场");
        assert_eq!(
            sensitive_data
                .decrypt(&confirmation.service_location_encrypted)
                .unwrap(),
            "客户现场"
        );
        assert_eq!(
            confirmation.service_location_fingerprint,
            ServiceFulfillment::service_location_fingerprint("客户现场", fingerprint_key)
        );
    }

    /// 确认路径登记凭证并完成业务确认，不启动审批。
    #[test]
    fn confirm_does_not_start_approval() {
        let production = include_str!("service_fulfillment_confirm.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("confirm_service_fulfillment_with_assets"));
        assert!(production.contains("apply_confirmation"));
        assert!(production.contains("pending_assets.persist"));
        assert!(!production.contains("prepare_start"));
        assert!(!production.contains("start_approval"));
        assert!(!production.contains("ServiceFulfillmentAdapter"));
        assert!(!production.contains("load_published_graph"));
    }

    /// 占位值、空白与凭证元数据规则已下沉到实体层，Service 不得残留镜像规则。
    #[test]
    fn evidence_rules_do_not_stay_in_service() {
        let production = include_str!("service_fulfillment_confirm.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("normalize_actual_service_location"));
        assert!(!production.contains("validate_service_evidence_metadata"));
        assert!(!production.contains("SERVICE_LOCATION_PLACEHOLDER"));
        assert!(!production.contains("matches!(content_type"));
        assert!(production.contains("ActualServiceLocation::parse"));
        assert!(production.contains("ServiceEvidencePolicy::validate"));
    }
}
