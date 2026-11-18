use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::supplier_api;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BulkJobExt, SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
use persistence_core::Transactional;

use super::SupplierApiReadService;
use super::dto::SupplierConnectionJobView;
use crate::{Error, Result};
impl SupplierApiReadService {
    /// 查询连接下健康检查或目录同步后台任务的当前终态/进度。
    ///
    /// # 参数
    /// * `connection_id` - 连接主键。
    /// * `job_id` - 后台任务主键。
    /// * `actor` - 当前认证账号，按真实连接来源核对完整读取资格。
    ///
    /// # 返回
    /// 返回任务终态与进度视图。
    ///
    /// # 错误
    /// 任务不存在或不属于指定连接时返回 `NotFound`。仓储读取失败时返回对应错误。
    pub async fn connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionJobView> {
        let (db, authorization, actor) = (self.db.clone(), self.authorization.clone(), actor.clone());
        let (connection_id, job_id) = (connection_id.to_string(), job_id.to_string());
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &actor,
                            &supplier_api::CONNECTION_JOB_DETAIL,
                            AccessTarget::record(&connection_id)?,
                            executor,
                        )
                        .await?;

                    let job = db
                        .background_jobs()
                        .find_supplier_connection_job(
                            &connection_id,
                            &job_id,
                            &[SUPPLIER_HEALTH_CHECK_JOB_TYPE, SUPPLIER_CATALOG_SYNC_JOB_TYPE],
                            executor,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound("连接后台任务不存在".to_string()))?;
                    Ok(job_view(job))
                })
            })
            .await
    }
}
pub(crate) fn connection_failure_note(code: Option<&str>, fallback: &'static str) -> &'static str {
    match code {
        Some("JD_VOP_TOKEN_UNAVAILABLE") => "无法使用供应商连接的授权，请联系管理员核对当前授权",
        Some("TOKEN_EXPIRED") => "供应商连接的授权已失效，请联系管理员检查并更新授权",
        Some("TECHNICAL_CONFIG_CHANGED" | "CATALOG_CONTEXT_CHANGED") => {
            "供应商连接设置在处理期间发生变更，请核对当前设置和任务进度后再决定是否重新发起"
        },
        Some("CATALOG_SYNC_ADAPTER_UNAVAILABLE") => "当前供应商连接不支持商品同步，请联系管理员核对连接设置",
        Some("SUPPLIER_CONNECTOR_UNAVAILABLE") => "当前供应商连接不可用，请联系管理员核对连接设置",
        Some("CATALOG_DATA_INVALID" | "DGSS_MAPPING" | "JD_VOP_MAPPING_INVALID") => {
            "供应商商品资料不完整或不匹配，请核对商品及规格资料"
        },
        Some("CATALOG_SCOPE_PROOF_REQUIRED") => "无法确认供应商授权的商品范围，请核对授权；未按缺项撤回商品",
        Some("OUTCOME_UNKNOWN") => "本次处理结果尚未确认，请先查询任务状态，勿重复发起",
        _ => fallback,
    }
}

pub(super) fn connection_job_failure_note(summary: &str) -> String {
    let code = summary.split_once(':').map(|(code, _)| code.trim());
    connection_failure_note(code, "供应商连接任务未完成，请查看任务进度；如仍失败，请联系管理员").into()
}

fn job_view(job: BackgroundJob) -> SupplierConnectionJobView {
    let error_summary = job.error_summary.as_deref().map(connection_job_failure_note);
    SupplierConnectionJobView {
        job_id: job.base.id,
        job_no: job.job_no,
        action: job.domain_job_type.unwrap_or_default(),
        status: job.status,
        total: job.total_count,
        processed: job.processed_count,
        succeeded: job.success_count,
        failed: job.failed_count,
        error_summary,
        created_at: job.base.created_at,
        finished_at: job.finished_at.map(|at| at.unix_secs() as u64),
    }
}
