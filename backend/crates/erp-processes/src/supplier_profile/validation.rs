//! 供应商资料输入校验、附件引用解析与事务恢复。

use std::collections::HashSet;

use erp_core::ids::PartyId;
use erp_party::PartyExt;
use erp_supplier::{
    QualificationAttachmentSensitivity, SaveSupplierProfileRequest, SupplierProfileMutationView,
    SupplierProfileQualificationInput, SupplierQualificationSelection, command_view,
    validate_profile_selection,
};
use erp_support::{FileAssetExt, PendingAttachmentBatch, SensitivityClass};
use persistence_core::NoTransaction;

use super::{SupplierProfileService, SupplierProfileWithAssetsResult};
use crate::{Error, Result};

impl SupplierProfileService {
    /// 事务失败时查询同幂等键结果；并发首请求已提交时返回该稳定结果。
    ///
    /// # 参数
    /// * `transaction_result` - 刚结束的业务事务结果。
    /// * `intended_result` - 事务成功时要返回的视图。
    /// * `idempotency_key` - 根资料命令幂等键。
    /// * `operation` - 期望的命令操作名，用于核对重放。
    /// * `supplier_id` - 更新时的供应商 ID；创建时为 `None`。
    /// * `request_fingerprint` - 本次请求摘要。
    ///
    /// # 返回
    /// 事务成功时返回原视图且 `assets_committed` 为 `true`。事务失败但同键同参命令已存在时返回该命令视图；只有原错误是 `OutcomeUnknown` 时把 `assets_committed` 标为可能已提交。
    ///
    /// # 错误
    /// 幂等记录查询失败时返回该错误。已有命令与本次操作、供应商或摘要不一致时返回 `ConflictError`。没有可恢复命令时返回原事务错误。
    pub(super) async fn resolve_transaction_result_with_assets(
        &self,
        transaction_result: Result<()>,
        intended_result: SupplierProfileMutationView,
        idempotency_key: &str,
        operation: &str,
        supplier_id: Option<&str>,
        request_fingerprint: &str,
    ) -> Result<SupplierProfileWithAssetsResult> {
        match transaction_result {
            Ok(()) => Ok(SupplierProfileWithAssetsResult { view: intended_result, assets_committed: true }),
            Err(error) => {
                let assets_may_be_committed = matches!(&error, Error::OutcomeUnknown(_));
                match self.command_record(idempotency_key).await? {
                    Some(command) => {
                        command
                            .ensure_replayable(operation, supplier_id, request_fingerprint)
                            .map_err(|e| Error::ConflictError(e.to_string()))?;
                        Ok(SupplierProfileWithAssetsResult {
                            view: command_view(command),
                            assets_committed: assets_may_be_committed,
                        })
                    },
                    None => Err(error),
                }
            },
        }
    }

    /// 校验签约或付款主体存在且启用。
    ///
    /// # 参数
    /// * `party_id` - 待引用的企业主体 ID
    ///
    /// # 返回
    /// 主体存在且启用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 主体不存在、已停用或仓储查询失败时返回错误。
    pub(super) async fn ensure_party_active(&self, party_id: &PartyId) -> Result<()> {
        let party = self
            .db
            .party()
            .party(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("签约或付款主体不存在".to_string()))?;
        if party.company_profile.is_none() {
            return Err(Error::ValidationError("签约和付款主体必须是已维护的公司主体".into()));
        }
        if !party.is_active() {
            return Err(Error::BusinessLogicError("签约或付款主体已停用".to_string()));
        }
        Ok(())
    }

    /// 校验资质附件存在且敏感级别符合附件用途。
    ///
    /// # 参数
    /// * `qualifications` - 根资料命令提交的资质集合
    /// * `pending_assets` - 同命令待登记的文件资产
    ///
    /// # 返回
    /// 全部附件存在且满足资质类型最低敏感级别时返回 `Ok(())`。
    ///
    /// # 错误
    /// 附件不存在、敏感级别不足或仓储查询失败时返回错误。
    pub(super) async fn ensure_attachment_references(
        &self,
        qualifications: &[SupplierProfileQualificationInput],
        pending_assets: &dyn PendingAttachmentBatch,
    ) -> Result<()> {
        for qualification in qualifications {
            let Some(attachment_id) = qualification.attachment_id.as_ref() else {
                continue;
            };
            let sensitivity = match pending_assets.sensitivity(attachment_id) {
                Some(sensitivity) => sensitivity,
                None => {
                    self.db
                        .file_assets()
                        .find_by_id(attachment_id.as_ref(), &mut NoTransaction)
                        .await?
                        .ok_or_else(|| Error::NotFound("资质附件不存在，请先上传文件".to_string()))?
                        .sensitivity_class
                },
            };
            let sensitivity = match sensitivity {
                SensitivityClass::General => QualificationAttachmentSensitivity::General,
                SensitivityClass::Sensitive => QualificationAttachmentSensitivity::Sensitive,
                SensitivityClass::HighlySensitive => QualificationAttachmentSensitivity::HighlySensitive,
            };
            if !qualification.qualification_type.accepts_attachment_sensitivity(sensitivity) {
                return Err(Error::ValidationError("资质附件敏感级别不足，请按敏感资料重新上传".to_string()));
            }
        }
        Ok(())
    }

    /// 校验根资料能力与资质选择关系。
    ///
    /// # 参数
    /// * `req` - 已通过 DTO 格式校验的根资料命令
    ///
    /// # 返回
    /// 能力、资质身份唯一且资质仅引用已勾选能力时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一纯领域选择规则不满足时返回校验错误。
    pub(super) fn ensure_unique_inputs(&self, req: &SaveSupplierProfileRequest) -> Result<()> {
        let qualifications: Vec<SupplierQualificationSelection<'_>> = req
            .qualifications
            .iter()
            .map(|qualification| SupplierQualificationSelection {
                qualification_type: qualification.qualification_type,
                certificate_no: &qualification.certificate_no,
                capability_codes: &qualification.capability_codes,
            })
            .collect();
        validate_profile_selection(&req.capability_codes, &qualifications)
            .map_err(|error| Error::ValidationError(error.to_string()))
    }
}

/// 解析供应商根命令中的临时资质文件引用，并把命中的临时 ID 换成正式 ID。
///
/// # 参数
/// * `req` - 根资料请求，资质附件 ID 会被就地替换。
/// * `pending_assets` - 同一次命令待登记的文件。
///
/// # 返回
/// 本次命令实际用到的临时文件 ID。
///
/// # 错误
/// 临时引用无法解析时返回 `pending_assets` 的错误。
pub(super) fn resolve_supplier_file_references(
    req: &mut SaveSupplierProfileRequest,
    pending_assets: &dyn PendingAttachmentBatch,
) -> Result<HashSet<String>> {
    let mut used = HashSet::new();
    for qualification in &mut req.qualifications {
        if let Some(attachment_id) = qualification.attachment_id.as_mut() {
            pending_assets.resolve_id(attachment_id, &mut used)?;
        }
    }
    Ok(used)
}

/// 校验敏感事实行归属于令牌限定供应商的 Party。
///
/// # 参数
/// * `actual` - 事实行上的主体 ID。
/// * `expected` - 令牌供应商的主体 ID。
///
/// # 返回
/// 两者相同时无额外值。
///
/// # 错误
/// 主体不一致时返回校验错误。
pub(super) fn ensure_sensitive_party(actual: &PartyId, expected: &PartyId) -> Result<()> {
    if actual != expected {
        return Err(Error::ValidationError("敏感字段令牌与供应商不匹配".to_string()));
    }
    Ok(())
}
