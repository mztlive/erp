use application_core::AuditActor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use erp_core::common::time::Instant;
use erp_supplier_catalog::supply_governance::offering::batch::{BatchRequest, Targeted};
use erp_supply::dto::supplier_offering::{
    ReviseSupplierOfferingRequest, UpdateSupplierOfferingAvailabilityRequest,
};
use persistence_core::{Executor, Transactional};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::{BusinessTransfers, TransferKind, TransferResult};
use crate::sales_inquiry_import::SalesInquiryImportProcess;
use crate::{Error, Result};

pub(super) fn decode<T: DeserializeOwned>(input: &Value) -> Result<T> {
    serde_json::from_value(input.clone()).map_err(|_| Error::ValidationError("任务参数格式无效".into()))
}
pub(super) fn required<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::ValidationError(format!("缺少参数 {key}")))
}
pub(super) fn version(input: &Value) -> Result<u64> {
    input["expected_version"]
        .as_u64()
        .ok_or_else(|| Error::ValidationError("缺少当前业务版本，请刷新后提交".into()))
}
impl BusinessTransfers {
    pub(super) async fn prepare_import(
        &self,
        kind: TransferKind,
        input: Value,
        actor: &AuditActor,
    ) -> Result<Value> {
        self.authorize_import(kind, &input, &Value::Null, actor).await?;
        if matches!(kind, TransferKind::InquiryPreview | TransferKind::InquiryCreationPreview) {
            let file = required(&input, "file_name")?;
            let bytes = STANDARD
                .decode(required(&input, "content_base64")?)
                .map_err(|_| Error::ValidationError("文件编码无效".into()))?;
            if bytes.is_empty()
                || bytes.len() > 5 * 1024 * 1024
                || !file.to_ascii_lowercase().ends_with(".xlsx")
            {
                return Err(Error::ValidationError("请选择不超过 5 MiB 的 XLSX 文件".into()));
            }
        }
        Ok(input)
    }
    pub(super) async fn authorize_import(
        &self,
        kind: TransferKind,
        input: &Value,
        authorization: &Value,
        actor: &AuditActor,
    ) -> Result<()> {
        if authorization["expires_at"].as_i64().is_some_and(|at| at <= Instant::now().unix_secs()) {
            return Err(Error::ConflictError("导入预览已过期，请重新上传".into()));
        }
        match kind {
            TransferKind::InquiryPreview | TransferKind::InquiryCreationPreview => {
                let service = self.services.inquiry.clone();
                let actor = actor.clone();
                let input = input.clone();
                self.db
                    .client()
                    .with_transaction(move |executor| {
                        Box::pin(
                            async move { authorize_inquiry(&service, kind, &input, &actor, executor).await },
                        )
                    })
                    .await?;
            },
            TransferKind::SupplyBatch => {
                self.supply_batch(input, actor, true).await?;
            },
            TransferKind::PortalBatch => return self.portal_authorize(input, actor).await,
            TransferKind::SupplierQuotePreview => {
                self.quote_context(input, actor).await?;
            },
            _ => return Err(Error::ValidationError("不支持的导入任务".into())),
        }
        Ok(())
    }
    pub(super) async fn import(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<TransferResult> {
        let service = self.services.inquiry.clone();
        let result = match kind {
            TransferKind::InquiryPreview | TransferKind::InquiryCreationPreview => {
                let bytes = STANDARD
                    .decode(required(input, "content_base64")?)
                    .map_err(|_| Error::ValidationError("文件编码无效".into()))?;
                let file = required(input, "file_name")?;
                let preview = if kind == TransferKind::InquiryPreview {
                    service
                        .preview(actor, required(input, "inquiry_id")?, version(input)?, file, bytes)
                        .await?
                } else {
                    service.preview_creation(actor, required(input, "customer_id")?, file, bytes).await?
                };
                serde_json::to_value(preview).map_err(json_error)?
            },
            TransferKind::SupplyBatch => self.supply_batch(input, actor, false).await?,
            TransferKind::PortalBatch => self.portal_batch(input, actor).await?,
            TransferKind::SupplierQuotePreview => self.quote_preview(input, actor).await?,
            _ => return Err(Error::ValidationError("不支持的导入任务".into())),
        };

        let authorization = json!({"expires_at":result.get("expires_at")});
        TransferResult::file(
            "处理结果.json".into(),
            "application/json",
            serde_json::to_vec(&result).map_err(json_error)?,
            authorization,
        )
    }
    pub(super) async fn supply_batch(
        &self,
        input: &Value,
        actor: &AuditActor,
        validate: bool,
    ) -> Result<Value> {
        let service = self.services.offerings.clone();
        let mut request = input.clone();
        request["validate_only"] = json!(validate);
        let result = match required(input, "mode")? {
            "revise" => {
                service
                    .batch(decode::<BatchRequest<Targeted<ReviseSupplierOfferingRequest>>>(&request)?, actor)
                    .await?
            },
            "availability" => {
                service
                    .batch(
                        decode::<BatchRequest<Targeted<UpdateSupplierOfferingAvailabilityRequest>>>(
                            &request,
                        )?,
                        actor,
                    )
                    .await?
            },
            _ => {
                return Err(Error::ValidationError(
                    "仅支持批量调价或更新可供情况，批量建档请使用商品导入".into(),
                ));
            },
        };
        serde_json::to_value(result).map_err(json_error)
    }
}

/// 使用提交或读取用例的原快照检查询价目标及其业务版本。
/// # 参数
/// service、kind、input 为固定询价任务和原始业务参数，actor、executor 为本次身份与原执行器。
/// # 返回
/// 当前目标或创建归属以及业务版本满足时成功。
/// # 错误
/// 非询价任务、无权限、对象缺失或业务版本变化时拒绝。
pub(super) async fn authorize_inquiry(
    service: &SalesInquiryImportProcess,
    kind: TransferKind,
    input: &Value,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    match kind {
        TransferKind::InquiryPreview => service
            .require_import(actor, required(input, "inquiry_id")?, executor)
            .await?
            .ensure_version(version(input)?)?,
        TransferKind::InquiryCreationPreview => {
            service.creation_header(actor, required(input, "customer_id")?, executor).await?;
        },
        _ => return Err(Error::ValidationError("该任务不是询价导入预览".into())),
    }
    Ok(())
}

fn json_error(_: serde_json::Error) -> Error {
    Error::Internal("任务结果无法编码".into())
}
