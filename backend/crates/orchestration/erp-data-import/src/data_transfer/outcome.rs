//! 根据原批量回执记录成功、失败与待确认；不以任务完成覆盖行失败。
use erp_core::common::time::Instant;
use erp_support::{BackgroundJob, BackgroundJobItem, ItemStatus};
use serde_json::Value;

use super::TransferResult;
use super::submit::row_identity;
use crate::{Error, Result};
pub(super) fn apply(
    job: &mut BackgroundJob,
    items: &mut [BackgroundJobItem],
    result: &TransferResult,
) -> Result<()> {
    let decoded = if result.content_type == "application/json" {
        Some(
            serde_json::from_slice::<Value>(&result.bytes()?)
                .map_err(|_| Error::Internal("处理结果格式无效".into()))?,
        )
    } else {
        None
    };
    let rows = decoded.as_ref().and_then(|r| r["rows"].as_array());
    let mut success = 0;
    let mut failed = 0;
    for item in items {
        let row = rows.and_then(|rows| {
            rows.iter().find(|r| {
                r["row_id"]
                    .as_str()
                    .is_some_and(|id| row_identity(id) == item.object_id.as_deref().unwrap_or_default())
            })
        });
        if item.object_type.as_deref() == Some("data_transfer_row") && row.is_none() {
            return Err(Error::Internal("逐行回执缺失，须按原命令恢复".into()));
        }
        let status = row.and_then(|r| r["status"].as_str()).unwrap_or("SUCCEEDED");
        let ok = matches!(status, "SUCCEEDED" | "succeeded" | "replayed");
        let unknown = matches!(status, "UNKNOWN" | "unknown");
        let message = message(ok, unknown);
        item.record_result(
            if ok { ItemStatus::Success } else { ItemStatus::Failed },
            if ok {
                None
            } else if unknown {
                Some("outcome_unknown".into())
            } else {
                Some("row_failed".into())
            },
            Some(message.into()),
            None,
            None,
        )?;
        if ok {
            success += 1;
        } else {
            failed += 1;
        }
    }
    if success + failed != job.total_count {
        return Err(Error::Internal("任务结果行数不一致".into()));
    }
    job.record_import_result_batch(success, 0, failed, true, Instant::now())?;
    Ok(())
}

fn message(ok: bool, unknown: bool) -> &'static str {
    if ok {
        "处理完成"
    } else if unknown {
        "提交结果待核对，请使用原操作号恢复，勿改内容重复提交"
    } else {
        "此行未完成，请下载结果核对原因后重新校验"
    }
}

#[cfg(test)]
mod tests {
    use erp_support::JobStatus;
    use serde_json::json;

    use super::*;
    use crate::data_transfer::submit::test_batch;
    fn result(value: Value) -> TransferResult {
        TransferResult::file(
            "result.json".into(),
            "application/json",
            serde_json::to_vec(&value).unwrap(),
            Value::Null,
        )
        .unwrap()
    }
    #[test]
    fn mixed_result_keeps_failure_and_unknown_recovery_marker() {
        let (mut job, mut items) = test_batch();
        job.start(Instant::now()).unwrap();
        apply(
            &mut job,
            &mut items,
            &result(
                json!({"rows":[{"row_id":"r1","status":"SUCCEEDED"},{"row_id":"r2","status":"UNKNOWN"}]}),
            ),
        )
        .unwrap();
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert_eq!((job.success_count, job.failed_count, job.processed_count), (1, 1, 2));
        assert!(job.finished_at.is_some());
        assert_eq!(items[1].result_code.as_deref(), Some("outcome_unknown"));
    }
    #[test]
    fn missing_result_is_not_counted_as_success() {
        let (mut job, mut items) = test_batch();
        job.start(Instant::now()).unwrap();
        assert!(apply(&mut job, &mut items, &result(json!({"rows":[]}))).is_err());
        assert_eq!(job.processed_count, 0);
    }
    #[test]
    fn all_failed_is_terminal_failed_and_replayed_rows_succeed() {
        for (status, expected) in
            [("validation_failed", JobStatus::Failed), ("replayed", JobStatus::Succeeded)]
        {
            let (mut job, mut items) = test_batch();
            job.start(Instant::now()).unwrap();
            apply(
                &mut job,
                &mut items,
                &result(json!({"rows":[{"row_id":"r1","status":status},{"row_id":"r2","status":status}]})),
            )
            .unwrap();
            assert_eq!(job.status, expected);
        }
    }
}
