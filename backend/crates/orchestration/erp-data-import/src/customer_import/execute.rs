//! 有界认领与逐行建档。成功行与任务结果在同一事务提交。
use std::time::Duration;

use application_core::AuditActor;
use async_trait::async_trait;
use background_runtime::{BackgroundTaskAdapter, TaskResult};
use erp_core::AccountKind;
use erp_core::common::time::{BusinessDate, Instant};
use erp_customer::{CustomerProfileMutationView, SaveCustomerProfileRequest};
use erp_master_data::Error as ProfileError;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BackgroundJobId, BackgroundJobItem, BulkJobExt, ItemStatus, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, ReadExecutor, Transactional};

use super::source::source_row;
use super::{CustomerImportProcess, CustomerImportRow, DOMAIN};
use crate::background::import_budget::import_budget;
use crate::{Error, Result};

#[async_trait]
impl BackgroundTaskAdapter for CustomerImportProcess {
    fn name(&self) -> &'static str {
        "customer_import"
    }

    async fn run_due(&self) -> TaskResult<usize> {
        let jobs = self.sources.poll_jobs(&self.db, DOMAIN).await?;
        let count = jobs.len();
        for mut job in jobs {
            if let Err(error) = self.execute(&mut job).await
                && !matches!(error, Error::ConflictError(_) | Error::TransientTransaction(_))
            {
                tracing::warn!(job_id = %job.base.id, "客户导入暂未完成，将按原任务恢复");
            }
        }
        Ok(count)
    }
}

impl CustomerImportProcess {
    async fn execute(&self, job: &mut BackgroundJob) -> Result<()> {
        let now = Instant::now();
        if !job.can_resume_import(now, 120) {
            return Ok(());
        }
        if job.status == JobStatus::Pending {
            job.start(now)?;
        }
        job.last_progress_at = Some(now);
        self.db.background_jobs().update(job, &mut ReadExecutor::default()).await?;

        let source = match self
            .sources
            .get_or_load(job, || async {
                tokio::time::timeout(Duration::from_secs(60), self.read_source(job))
                    .await
                    .map_err(|_| Error::Internal("读取客户导入内容超时".into()))?
            })
            .await
        {
            Ok(source) => source,
            Err(_) => return self.fail_job(job, "读取导入文件失败，请使用原文件重新提交").await,
        };

        let items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(&job.base.id), &mut ReadExecutor::default())
            .await?;
        let mut budget = import_budget();
        for item in items.into_iter().filter(|item| item.status.is_none()) {
            let row = source_row(&source, &item)?;
            match self.commit_row(job, item, row).await {
                Ok(Some(updated)) => *job = updated,
                Ok(None) => return Ok(()),
                Err(Error::Forbidden(_) | Error::Unauthenticated(_)) => {
                    return self.fail_job(job, "提交人或客户建档权限已失效，请恢复权限后处理剩余行").await;
                },
                Err(error) => return Err(error),
            }
            if budget.after_row() {
                break;
            }
        }

        if job.finished_at.is_some() {
            self.sources.remove(&job.base.id).await;
        } else {
            job.last_progress_at = None;
            self.db.background_jobs().update(job, &mut ReadExecutor::default()).await?;
        }
        Ok(())
    }

    async fn fail_job(&self, job: &mut BackgroundJob, message: &str) -> Result<()> {
        job.mark_failed(Some(message.into()), Instant::now())?;
        self.db.background_jobs().update(job, &mut ReadExecutor::default()).await?;
        self.sources.remove(&job.base.id).await;
        Ok(())
    }

    async fn commit_row(
        &self,
        job: &BackgroundJob,
        item: BackgroundJobItem,
        row: &CustomerImportRow,
    ) -> Result<Option<BackgroundJob>> {
        let actor = AuditActor::new(job.requested_by.clone(), job.requested_by.clone(), AccountKind::Admin);
        if let Some(error) = &row.error {
            return self.record_rejection(job, item, error.clone(), actor).await;
        }

        let request_key = format!("customer-import:{}:{}", job.base.id, row.row_number);
        let request = profile_request(row, request_key, BusinessDate::today());
        let db = self.db.clone();
        let profile = self.profile.clone();
        let operator = actor.clone();
        let job_copy = job.clone();
        let item_copy = item.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Some(current) = current_job(&db, &job_copy, executor).await? else {
                        return Ok(None);
                    };
                    let created = profile
                        .create_in_transaction(request, &operator, executor)
                        .await
                        .map_err(RowError::from_create)?;
                    let committed = store_result(&db, current, item_copy, Ok(created), executor).await?;
                    Ok::<_, RowError>(Some(committed))
                })
            })
            .await;

        match result {
            Ok(updated) => Ok(updated),
            // Failed creates have rolled back. Record the rejection only if this worker still owns the job.
            Err(RowError::Rejected(message)) => self.record_rejection(job, item, message, actor).await,
            Err(RowError::Retry(error)) => Err(error),
        }
    }

    async fn record_rejection(
        &self,
        job: &BackgroundJob,
        item: BackgroundJobItem,
        message: String,
        actor: AuditActor,
    ) -> Result<Option<BackgroundJob>> {
        let db = self.db.clone();
        let profile = self.profile.clone();
        let job = job.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Some(current) = current_job(&db, &job, executor).await? else {
                        return Ok(None);
                    };
                    profile.require_create(&actor, executor).await.map_err(|error| match error {
                        ProfileError::ValidationError(message) => Error::Forbidden(message),
                        other => other.into(),
                    })?;
                    let committed = store_result(&db, current, item, Err(message), executor).await?;
                    Ok(Some(committed))
                })
            })
            .await
    }
}

enum RowError {
    Rejected(String),
    Retry(Error),
}

impl RowError {
    fn from_create(error: ProfileError) -> Self {
        let message = match error {
            ProfileError::ValidationError(message)
            | ProfileError::ConflictError(message)
            | ProfileError::BusinessLogicError(message) => message,
            ProfileError::InvalidInput(issues) => {
                issues.fields().values().cloned().collect::<Vec<_>>().join("；")
            },
            ProfileError::Logic(error) => error.to_string(),
            other => return Self::Retry(other.into()),
        };
        Self::Rejected(message)
    }
}

impl From<Error> for RowError {
    fn from(error: Error) -> Self {
        Self::Retry(error)
    }
}

impl From<persistence_core::Error> for RowError {
    fn from(error: persistence_core::Error) -> Self {
        Self::Retry(error.into())
    }
}

fn profile_request(
    row: &CustomerImportRow,
    idempotency_key: String,
    date: BusinessDate,
) -> SaveCustomerProfileRequest {
    SaveCustomerProfileRequest {
        idempotency_key,
        reuse_existing_party: false,
        expected_party_version: None,
        expected_customer_version: None,
        legal_name: row.legal_name.clone(),
        short_name: row.short_name.clone(),
        unified_credit_code: row.unified_credit_code.clone(),
        default_payment_term_id: None,
        status: None,
        owner_user_id: None,
        contacts: None,
        addresses: None,
        bank_accounts: None,
        effective_from: date,
        change_reason: "批量导入建档".into(),
    }
}

async fn current_job(
    db: &Database,
    expected: &BackgroundJob,
    executor: &mut dyn Executor,
) -> Result<Option<BackgroundJob>> {
    let job = db
        .background_jobs()
        .find_by_id(&expected.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
    Ok((job.base.version == expected.base.version
        && job.finished_at.is_none()
        && matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded))
    .then_some(job))
}

async fn store_result(
    db: &Database,
    mut job: BackgroundJob,
    mut item: BackgroundJobItem,
    result: std::result::Result<CustomerProfileMutationView, String>,
    executor: &mut dyn Executor,
) -> Result<BackgroundJob> {
    record(&mut job, &mut item, result)?;
    db.background_job_items().update(&mut item, executor).await?;
    db.background_jobs().update(&mut job, executor).await?;
    Ok(job)
}

fn record(
    job: &mut BackgroundJob,
    item: &mut BackgroundJobItem,
    result: std::result::Result<CustomerProfileMutationView, String>,
) -> Result<()> {
    let success = result.is_ok();
    match result {
        Ok(view) => item.record_result(
            ItemStatus::Success,
            None,
            Some(format!("已建档，客户编号 {}", view.customer_no)),
            Some("customer".into()),
            Some(view.customer_id),
        )?,
        Err(message) => item.record_result(
            ItemStatus::Failed,
            Some("import_failed".into()),
            Some(message.trim().chars().take(160).collect::<String>())
                .filter(|message| !message.is_empty())
                .or(Some("资料校验失败，请核对原表".into())),
            None,
            None,
        )?,
    };
    job.record_import_result_batch(
        u64::from(success),
        0,
        u64::from(!success),
        job.processed_count + 1 == job.total_count,
        Instant::now(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_conflicts_reject_the_row_and_unknown_or_denied_transactions_are_not_row_results() {
        assert!(matches!(
            RowError::from_create(ProfileError::ConflictError("主体已存在".into())),
            RowError::Rejected(message) if message == "主体已存在"
        ));
        assert!(matches!(
            RowError::from_create(ProfileError::Forbidden("权限已失效".into())),
            RowError::Retry(Error::Forbidden(_))
        ));
        let unknown = persistence_core::Error::CommitOutcomeUnknown(mongodb::error::Error::custom("commit"));
        assert!(matches!(RowError::from(unknown), RowError::Retry(Error::OutcomeUnknown(_))));
    }

    #[test]
    fn preserves_credit_code_and_never_confirms_existing_identity_or_sets_owner() {
        let row = CustomerImportRow {
            row_number: 9,
            legal_name: "上海公司".into(),
            short_name: Some("上海".into()),
            unified_credit_code: Some("001234567890123456".into()),
            error: None,
        };
        let date = BusinessDate::from_ymd(2026, 11, 23).unwrap();
        let request = profile_request(&row, "customer-import:job:9".into(), date);
        assert_eq!(request.effective_from, date);
        assert_eq!(request.unified_credit_code, row.unified_credit_code);
        assert!(!request.reuse_existing_party);
        assert!(request.owner_user_id.is_none());
        assert!(request.status.is_none());
    }
}
