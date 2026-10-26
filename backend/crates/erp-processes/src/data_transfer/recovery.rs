//! 批量任务核对只读原命令回执，保留所有未获确定成功证据的结果。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::BackgroundJobId;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BackgroundJobItem, BulkJobExt, FileAssetExt, JobUpdate};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use serde_json::Value;

use super::submit::row_identity;
use super::{DataTransferProcess, TransferKind, TransferResult};
use crate::{Error, Result};

impl DataTransferProcess {
    /// 仅核对原批量任务中结果未知项，不重新执行业务命令。
    /// # 参数
    /// `id` 为原任务，`actor` 为原提交人当前身份。
    /// # 返回
    /// 已确认回执合并后的原任务结果。
    /// # 错误
    /// 非本人、不是批量任务、无结果未知项、过期或权限失效时拒绝。
    pub async fn recover(&self, id: &str, actor: &AuditActor) -> Result<TransferResult> {
        let job = self.owned_job(id, actor).await?;
        let source = self.source(&job).await?;
        if !matches!(source.kind, TransferKind::SupplyBatch | TransferKind::PortalBatch) {
            return Err(Error::ValidationError("该任务不支持批量结果核对".into()));
        }
        let mut result = self.result(id, actor).await?;
        let mut original: Value = serde_json::from_slice(&result.bytes()?)
            .map_err(|_| Error::Internal("批量结果格式无效".into()))?;
        if !has_unknown_rows(&original) {
            return Err(Error::ConflictError("没有需要核对的原操作结果".into()));
        }
        let fresh = self.handler.recover(source.kind, &source.input, actor).await?;
        let confirmed = merge_confirmed(&mut original, &fresh)?;
        if confirmed.is_empty() {
            return Ok(result);
        }
        result.content_base64 = TransferResult::file(
            result.file_name.clone(),
            &result.content_type,
            serde_json::to_vec(&original).map_err(|_| Error::Internal("批量结果编码失败".into()))?,
            result.authorization.clone(),
        )?
        .content_base64;
        self.save_recovered(job, &result, &confirmed).await?;
        self.result(id, actor).await
    }

    async fn save_recovered(
        &self,
        mut job: BackgroundJob,
        result: &TransferResult,
        confirmed: &[RecoveredRow],
    ) -> Result<()> {
        let mut items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(&job.base.id), &mut NoTransaction)
            .await?;
        apply_recovered(&mut job, &mut items, confirmed, Instant::now())?;
        let mut asset = self
            .save_private(
                &format!("data-transfer/{}/recovered-{}.enc", job.base.id, next_id()),
                result,
                &job.requested_by,
            )
            .await?;
        asset.expires_at = job.result_expires_at;
        job.update(JobUpdate {
            result_file_asset_id: Some(asset.base.id.clone().into()),
            result_expires_at: job.result_expires_at,
        });
        let db = self.db.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.file_assets().create(&asset, executor).await?;
                    for item in &mut items {
                        db.background_job_items().update(item, executor).await?;
                    }
                    db.background_jobs().update(&mut job, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await
    }
}

pub(super) fn has_unknown_rows(result: &Value) -> bool {
    result["rows"].as_array().into_iter().flatten().any(|row| is_unknown(&row["status"]))
}

fn is_unknown(status: &Value) -> bool {
    matches!(status.as_str(), Some("UNKNOWN" | "unknown"))
}

#[derive(Debug, PartialEq, Eq)]
struct RecoveredRow {
    id: String,
    succeeded: bool,
}

fn apply_recovered(
    job: &mut BackgroundJob,
    items: &mut [BackgroundJobItem],
    confirmed: &[RecoveredRow],
    now: Instant,
) -> Result<()> {
    let mut success = 0_u64;
    for row in confirmed {
        let item = items
            .iter_mut()
            .find(|item| item.object_id.as_deref() == Some(row_identity(&row.id).as_str()))
            .ok_or_else(|| Error::Internal("待确认结果与原任务明细不一致".into()))?;
        if row.succeeded {
            item.confirm_unknown_success()?;
            success += 1;
        } else {
            item.confirm_unknown_failure()?;
        }
    }
    if success > 0 {
        job.confirm_unknown_successes(success, now)?;
    }
    Ok(())
}

fn merge_confirmed(original: &mut Value, fresh: &Value) -> Result<Vec<RecoveredRow>> {
    let rows = original["rows"].as_array_mut().ok_or_else(|| Error::Internal("原批量结果缺少明细".into()))?;
    let mut confirmed = Vec::new();
    for row in rows.iter_mut().filter(|row| is_unknown(&row["status"])) {
        let id = row["row_id"].as_str().ok_or_else(|| Error::Internal("原批量结果缺少行身份".into()))?;
        let found = fresh["rows"].as_array().into_iter().flatten().find(|candidate| {
            candidate["row_id"] == id && (is_success(candidate) || is_sealed_rejection(candidate))
        });
        if let Some(found) = found {
            confirmed.push(RecoveredRow { id: id.into(), succeeded: is_success(found) });
            *row = found.clone();
        }
    }
    Ok(confirmed)
}

fn is_success(row: &Value) -> bool {
    matches!(row["status"].as_str(), Some("SUCCEEDED" | "succeeded" | "replayed"))
}

fn is_sealed_rejection(row: &Value) -> bool {
    row["status"] == "failed" && row["result"]["command_outcome"] == "rejected"
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn recovery_only_merges_original_unknown_rows_with_successful_receipts() {
        let mut original = json!({"rows":[{"row_id":"a","status":"UNKNOWN"},{"row_id":"b","status":"failed"},{"row_id":"c","status":"unknown"}]});
        let fresh = json!({"rows":[{"row_id":"a","status":"SUCCEEDED","result":{"id":"saved"}},{"row_id":"b","status":"SUCCEEDED"},{"row_id":"c","status":"READY"}]});
        assert_eq!(
            merge_confirmed(&mut original, &fresh).unwrap(),
            vec![RecoveredRow { id: "a".into(), succeeded: true }]
        );
        assert_eq!(original["rows"][0]["result"]["id"], "saved");
        assert_eq!(original["rows"][1]["status"], "failed");
        assert_eq!(original["rows"][2]["status"], "unknown");
        assert!(has_unknown_rows(&original));
    }

    #[test]
    fn recovered_receipts_update_real_task_items_counts_and_definite_rejections() {
        use erp_support::{ItemStatus, JobStatus};

        use super::super::outcome::apply;
        use super::super::submit::test_batch;
        let (mut job, mut items) = test_batch();
        job.start(Instant::from_unix_secs(100)).unwrap();
        let mut original =
            json!({"rows":[{"row_id":"r1","status":"UNKNOWN"},{"row_id":"r2","status":"UNKNOWN"}]});
        let file = TransferResult::file(
            "结果.json".into(),
            "application/json",
            serde_json::to_vec(&original).unwrap(),
            Value::Null,
        )
        .unwrap();
        apply(&mut job, &mut items, &file).unwrap();
        let fresh = json!({"rows":[{"row_id":"r1","status":"replayed","result":{"id":"saved"}},
            {"row_id":"r2","status":"failed","result":{"command_outcome":"rejected"}}]});
        let recovered = merge_confirmed(&mut original, &fresh).unwrap();
        apply_recovered(&mut job, &mut items, &recovered, Instant::now()).unwrap();
        assert_eq!((job.success_count, job.failed_count, job.processed_count), (1, 1, 2));
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert_eq!(items[0].status, Some(ItemStatus::Success));
        assert_eq!(items[1].result_code.as_deref(), Some("row_failed"));
        assert!(!has_unknown_rows(&original));
        assert!(merge_confirmed(&mut original, &fresh).unwrap().is_empty());
        assert!(apply_recovered(&mut job, &mut items, &recovered, Instant::now()).is_err());
    }
}
