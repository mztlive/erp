//! 本人文件任务的业务恢复信息；业务事实与当前授权均在读取时重新核验。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_sales::repository::extensions::SalesInquiryImportExt;
use erp_support::{BackgroundJob, BulkJobExt};
use persistence_core::NoTransaction;
use serde_json::{Value, json};

use super::imports::required;
use super::submit::DOMAIN;
use super::{
    BusinessTransfers, DataTransferProcess, TransferContext, TransferKind, TransferResult,
    TransferResumeState,
};
use crate::sales_inquiry::read::InquiryReadService;
use crate::sales_inquiry_import::SalesInquiryImportProcess;
use crate::{Error, Result};

impl DataTransferProcess {
    /// 从原私有输入读取本人任务类型与受控业务引用。
    /// # 参数
    /// `id` 为任务号，`actor` 为当前认证身份。
    /// # 返回
    /// 当前权限下的续办状态及取消能力。
    /// # 错误
    /// 非本人、历史输入不可恢复或当前授权失效时拒绝；新版过期上下文返回明确过期状态。
    pub async fn context(&self, id: &str, actor: &AuditActor) -> Result<TransferContext> {
        let job = self.owned_job(id, actor).await?;
        let mut source = self.context_input(&job).await?;
        let expired = source.expires_at <= Instant::now().unix_secs();
        source.input["__context_expires_at"] = json!(source.expires_at);
        let result =
            if source.kind.is_export() || expired { None } else { self.context_result(&job).await? };
        let mut view = self.handler.context(source.kind, &source.input, result.as_ref(), actor).await?;
        view.can_cancel = job.cancellation_allowed();
        view.can_recover = matches!(source.kind, TransferKind::SupplyBatch | TransferKind::PortalBatch)
            && job.finished_at.is_some()
            && result.as_ref().is_some_and(super::recovery::has_unknown_rows);
        if !expired && job.finished_at.is_none() && !source.kind.is_export() {
            view.state(TransferResumeState::Pending, "任务仍在后台处理，请完成后查看结果");
        }
        Ok(view)
    }

    pub(super) async fn owned_job(&self, id: &str, actor: &AuditActor) -> Result<BackgroundJob> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".into()))?;
        if job.requested_by != actor.id() || job.domain_job_type.as_deref() != Some(DOMAIN) {
            return Err(Error::Forbidden("只能读取本人提交任务的业务结果".into()));
        }
        Ok(job)
    }

    async fn context_result(&self, job: &BackgroundJob) -> Result<Option<Value>> {
        let Some(asset) = job.result_file_asset_id.as_ref() else {
            return Ok(None);
        };
        if job.finished_at.is_none() || job.result_expires_at.is_none_or(|at| at <= Instant::now()) {
            return Ok(None);
        }
        let result: TransferResult = self.read_private(asset.as_ref(), &job.requested_by).await?;
        if result.content_type != "application/json" {
            return Ok(None);
        }
        Ok(Some(
            serde_json::from_slice(&result.bytes()?)
                .map_err(|_| Error::Internal("任务结果格式无效".into()))?,
        ))
    }
}

impl BusinessTransfers {
    pub(super) async fn task_context(
        &self,
        kind: TransferKind,
        input: &Value,
        result: Option<&Value>,
        actor: &AuditActor,
    ) -> Result<TransferContext> {
        self.access(kind, input, actor).await?;
        if kind == TransferKind::PortalBatch {
            self.portal_context_access(input, actor).await?;
        }
        let mut view = TransferContext::from_input(kind, input);
        if input["__context_expires_at"].as_i64().is_some_and(|at| at <= Instant::now().unix_secs()) {
            let mut classification = json!({});
            for key in ["mode", "resource", "view"] {
                if let Some(value) = view.context.get(key) {
                    classification[key] = value.clone();
                }
            }
            view.context = classification;
            view.state(TransferResumeState::Expired, "任务文件及业务预览已过期，请返回原业务页重新提交");
            return Ok(view);
        }
        match kind {
            TransferKind::InquiryPreview | TransferKind::InquiryCreationPreview => {
                self.inquiry_context(&mut view, input, result, actor).await?;
            },
            TransferKind::SupplierQuotePreview => self.quote_resume(&mut view, input, result, actor).await?,
            TransferKind::SupplierQuoteArchive => {
                self.quote_context(input, actor).await?;
            },
            _ => {},
        }
        Ok(view)
    }

    async fn inquiry_context(
        &self,
        view: &mut TransferContext,
        input: &Value,
        result: Option<&Value>,
        actor: &AuditActor,
    ) -> Result<()> {
        let service = SalesInquiryImportProcess::new(self.db.clone(), self.rbac.clone());
        let current = if view.kind == TransferKind::InquiryCreationPreview {
            service.creation_header(actor, required(input, "customer_id")?, &mut NoTransaction).await?;
            None
        } else {
            let inquiry =
                service.require_import(actor, required(input, "inquiry_id")?, &mut NoTransaction).await?;
            view.context["inquiry_no"] = json!(inquiry.inquiry_no);
            Some(inquiry)
        };
        let Some(result) = result else {
            view.state(TransferResumeState::Unavailable, "尚无可继续处理的预览结果");
            return Ok(());
        };
        let preview = self
            .db
            .inquiry_import_previews()
            .find_by_id(required(result, "preview_id")?, &mut NoTransaction)
            .await?;
        let Some(preview) = preview else {
            view.state(TransferResumeState::Expired, "业务预览已过期，请重新上传文件");
            return Ok(());
        };
        if preview.actor_id != actor.id() {
            return Err(Error::Forbidden("无权读取此业务预览".into()));
        }
        view.preview_expires_at = Some(preview.expires_at);
        if let Some(confirmed) = preview.result {
            let inquiry = service.require_import(actor, &confirmed.inquiry_id, &mut NoTransaction).await?;
            view.context["inquiry_id"] = json!(confirmed.inquiry_id);
            view.context["inquiry_no"] = json!(inquiry.inquiry_no);
            view.state(TransferResumeState::Confirmed, "该预览已确认写入询价草稿，可查看原询价");
        } else if preview.expires_at <= Instant::now().unix_secs() {
            view.state(TransferResumeState::Expired, "业务预览已过期，请重新上传文件");
        } else if current.as_ref().is_some_and(|inquiry| inquiry.base.version != preview.expected_version) {
            view.state(TransferResumeState::Stale, "询价版本已变化，请返回业务页重新预览");
        } else if !preview.errors.is_empty() {
            view.state(TransferResumeState::Invalid, "预览存在校验问题，请修正文件后重新上传");
        } else {
            view.state(TransferResumeState::Ready, "预览已生成，核对后仍需确认写入询价草稿");
        }
        Ok(())
    }

    async fn quote_resume(
        &self,
        view: &mut TransferContext,
        input: &Value,
        result: Option<&Value>,
        actor: &AuditActor,
    ) -> Result<()> {
        let service = InquiryReadService::new(self.db.clone(), self.rbac.clone());
        let inquiry = required(input, "inquiry_id")?;
        let task = required(input, "task_id")?;
        let detail = serde_json::to_value(service.procurement_detail(actor, inquiry, task).await?)
            .map_err(|_| Error::Internal("询价资料无法编码".into()))?;
        view.context["inquiry_no"] = detail["inquiry_no"].clone();
        let Some(result) = result else {
            view.state(TransferResumeState::Unavailable, "尚无可继续处理的报价预览");
            return Ok(());
        };
        let prior = serde_json::to_value(service.internal_quotes(actor, inquiry, task).await?)
            .map_err(|_| Error::Internal("报价资料无法编码".into()))?;
        if quotes_confirmed(result, &prior) {
            view.state(TransferResumeState::Confirmed, "该预览中的报价已登记，可查看询价报价");
            return Ok(());
        }
        match self.quote_context(input, actor).await {
            Ok(original) => {
                view.context["requirement_version"] = original["requirement_version"].clone();
                view.context["feedback_round"] = original["feedback_round"]
                    .as_u64()
                    .and_then(|round| round.checked_add(1))
                    .map_or(Value::Null, |round| json!(round));
                if result["issues"].as_array().is_some_and(|issues| !issues.is_empty()) {
                    view.state(TransferResumeState::Invalid, "报价预览存在校验问题，请修正文件后重新上传");
                } else if result["rows"].as_array().is_none_or(Vec::is_empty) {
                    view.state(TransferResumeState::Invalid, "没有可登记的报价，请核对上传文件");
                } else {
                    view.state(TransferResumeState::Ready, "报价预览已生成，核对后仍需确认登记报价");
                }
            },
            Err(Error::ConflictError(_)) => {
                view.state(TransferResumeState::Stale, "询价或采购任务版本已变化，请重新导入报价")
            },
            Err(error) => return Err(error),
        }
        Ok(())
    }
}

fn quotes_confirmed(result: &Value, prior: &Value) -> bool {
    let Some(rows) = result["rows"].as_array().filter(|rows| !rows.is_empty()) else {
        return false;
    };
    rows.iter().all(|row| {
        let evidence = if row["status"] == "已导入" { &row["previous"] } else { &row["quote"] };
        prior.as_array().into_iter().flatten().any(|quote| {
            evidence["quote_id"].is_string()
                && quote["quote_id"] == evidence["quote_id"]
                && quote["revision_no"] == evidence["revision_no"]
                && quote["line_id"] == evidence["line_id"]
                && quote["supplier_id"] == evidence["supplier_id"]
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_requires_all_original_quote_revisions() {
        let result =
            json!({"rows":[{"quote":{"quote_id":"q","revision_no":2,"line_id":"l","supplier_id":"s"}}]});
        let prior = json!([{"quote_id":"q","revision_no":2,"line_id":"l","supplier_id":"s"}]);
        assert!(quotes_confirmed(&result, &prior));
        assert!(!quotes_confirmed(
            &result,
            &json!([{"quote_id":"q","revision_no":1,"line_id":"l","supplier_id":"s"}])
        ));
        assert!(!quotes_confirmed(&json!({"rows":[]}), &prior));
        assert!(!quotes_confirmed(&result, &json!([])));
    }

    #[test]
    fn already_imported_rows_use_previous_receipt_instead_of_unwritten_next_revision() {
        let prior = json!([{"quote_id":"q","revision_no":1,"line_id":"l","supplier_id":"s"}]);
        let result = json!({"rows":[{"status":"已导入", "previous":prior[0],
            "quote":{"quote_id":"q","revision_no":2,"line_id":"l","supplier_id":"s"}}]});
        assert!(quotes_confirmed(&result, &prior));
        assert!(!quotes_confirmed(&result, &json!([])));
    }
}
