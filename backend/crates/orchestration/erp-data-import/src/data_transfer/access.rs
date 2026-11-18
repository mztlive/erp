//! 任务读取先完成快照资格；对象存储读取和解密只在事务结束后执行。
use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::AccessTarget;
use erp_permissions::internal;
use erp_support::{BackgroundJob, BulkJobExt, JobType};
use futures_util::future::BoxFuture;
use persistence_core::{Executor, Transactional};

use super::context_input::ContextInput;
use super::submit::DOMAIN;
use super::{DataTransferProcess, TransferHandler};
use crate::{Error, Result};

pub(super) struct ReaderJob {
    pub job: BackgroundJob,
    pub context: Option<ContextInput>,
    pub owned: bool,
}

impl DataTransferProcess {
    // 在短快照先核验真实任务归属，再读取本人任务。
    //
    // id、actor 为真实任务和当前身份。
    //
    // 当前账号拥有的文件任务。
    //
    // 当前资格、归属、任务类型或事务失败时拒绝。
    pub(super) async fn owned_job(&self, id: &str, actor: &AuditActor) -> Result<BackgroundJob> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.reader_owner(&id, &actor, executor).await?;
                    process.transfer_job(&id, executor).await
                })
            })
            .await
    }

    // 在短快照完成任务和业务资格，不在事务内下载或解密。
    //
    // id、actor 为真实任务和当前身份，shared 允许当前获授权账号读取共享导出。
    //
    // 同快照授权的任务及可选轻量上下文。
    //
    // 当前资格、归属、任务类型或事务失败时拒绝。
    pub(super) async fn reader_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        shared: bool,
    ) -> Result<ReaderJob> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.reader(&id, &actor, shared, executor).await })
            })
            .await
    }

    // 原执行器先授权，再读取完整任务；共享分支只预读选择业务权限所需的持久化分类。
    //
    // id、actor 为真实任务和当前身份，shared 为共享导出入口，executor 为原快照。
    //
    // 当前资格允许读取的真实任务和分类事实。
    //
    // 当前身份、归属、功能资格、任务分类或读取失败时拒绝。
    pub(super) async fn reader(
        &self,
        id: &str,
        actor: &AuditActor,
        shared: bool,
        executor: &mut dyn Executor,
    ) -> Result<ReaderJob> {
        let owned = match self.reader_owner(id, actor, executor).await {
            Ok(()) => true,
            Err(Error::Forbidden(_)) if shared && actor.kind() == AccountKind::Admin => {
                self.job_access
                    .service
                    .permissions(actor, &[], executor)
                    .await?
                    .ok_or_else(|| Error::Forbidden("当前账号不能读取导出任务".into()))?;
                false
            },
            Err(error) => return Err(error),
        };

        let context = self.stored_context(id, executor).await?;
        if !owned && context.as_ref().is_none_or(|context| !context.kind.is_export()) {
            return Err(Error::Forbidden("该任务仅供原提交人读取，请返回原业务页重新提交".into()));
        }

        let job = if let Some(context) = &context {
            authorized_read(self.handler.as_ref(), context, actor, executor, |executor| {
                Box::pin(self.transfer_job(id, executor))
            })
            .await?
        } else {
            // 历史本人任务已通过真实记录授权；读取源后再检查其业务资格。
            self.transfer_job(id, executor).await?
        };
        if !owned && job.job_type != JobType::Export {
            return Err(Error::Forbidden("导入预览和批量处理结果仅供原提交人使用".into()));
        }
        Ok(ReaderJob { job, context, owned })
    }

    // 门户只查询不可变本人关系；内部账号由 identity 读取真实任务事实。
    async fn reader_owner(&self, id: &str, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        if actor.kind() == AccountKind::Supplier {
            let owned = self.db.bulk_job().is_requester(id, DOMAIN, actor.id(), executor).await?;
            return if owned {
                Ok(())
            } else {
                Err(Error::Forbidden("导入预览和批量处理结果仅供原提交人使用".into()))
            };
        }
        self.job_access
            .service
            .require(actor, &internal::DATA_TRANSFER_JOB_READ, AccessTarget::record(id)?, executor)
            .await?;
        Ok(())
    }

    // 外部读取结束后重验当前资格及原任务版本。
    //
    // original 为外部读取前的任务，actor 为交付身份。
    //
    // 当前资格与任务版本仍一致时成功。
    //
    // 撤权、任务变化或事务失败时拒绝交付。
    pub(super) async fn validate_reader(&self, original: &BackgroundJob, actor: &AuditActor) -> Result<()> {
        let current = self.reader_snapshot(&original.base.id, actor, true).await?;
        same_job(original, &current.job)
    }

    // 历史本人源只用于恢复缺失分类；结果读取前在新快照核验恢复出的业务资格。
    //
    // job、context 为原任务与已恢复分类，actor 为当前读取身份。
    //
    // 当前任务与业务资格仍允许读取时成功。
    //
    // 当前资格、任务版本或事务失效时拒绝。
    pub(super) async fn qualify_context(
        &self,
        job: &BackgroundJob,
        context: &ContextInput,
        actor: &AuditActor,
    ) -> Result<()> {
        let process = self.clone();
        let job = job.clone();
        let context = context.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.reader_owner(&job.base.id, &actor, executor).await?;
                    let current =
                        authorized_read(process.handler.as_ref(), &context, &actor, executor, |executor| {
                            Box::pin(process.transfer_job(&job.base.id, executor))
                        })
                        .await?;
                    same_job(&job, &current)
                })
            })
            .await
    }
}

// 资格失败时不轮询受保护读取 future，执行器始终沿用原快照。
async fn authorized_read<'a, T>(
    handler: &dyn TransferHandler,
    context: &ContextInput,
    actor: &AuditActor,
    executor: &'a mut dyn Executor,
    read: impl FnOnce(&'a mut dyn Executor) -> BoxFuture<'a, Result<T>>,
) -> Result<T> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("任务读取授权必须使用快照事务".into()));
    }
    handler.authorize_read(context.kind, &context.input, actor, executor).await?;
    read(executor).await
}

// 只向原任务及其原版本交付外部读取结果。
//
// original、current 为外部读取前后真实任务。
//
// 任务身份与版本一致时成功。
//
// 任一身份或版本发生变化时拒绝。
pub(super) fn same_job(original: &BackgroundJob, current: &BackgroundJob) -> Result<()> {
    if original.base.id != current.base.id || original.base.version != current.base.version {
        return Err(Error::ConflictError("文件任务已变化，请重新读取".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};
    use serde_json::{Value, json};

    use super::*;
    use crate::data_transfer::{TransferContext, TransferKind, TransferResult};

    #[derive(Default)]
    struct Handler {
        denied: bool,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl TransferHandler for Handler {
        async fn authorize_read(
            &self,
            kind: TransferKind,
            input: &Value,
            actor: &AuditActor,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            assert_eq!(kind, TransferKind::SalesOrders);
            assert_eq!(input["scope"], "own");
            assert_eq!(actor.id(), "reader");
            self.calls.lock().unwrap().push("authorize".into());
            if self.denied { Err(Error::Forbidden("业务资格已撤销".into())) } else { Ok(()) }
        }

        async fn authorize_submission(
            &self,
            _: TransferKind,
            _: &Value,
            _: &AuditActor,
            _: &mut dyn Executor,
        ) -> Result<()> {
            unreachable!()
        }
        async fn prepare(&self, _: TransferKind, _: Value, _: &AuditActor) -> Result<Value> {
            unreachable!()
        }
        async fn execute(&self, _: TransferKind, _: &Value, _: &AuditActor) -> Result<TransferResult> {
            unreachable!()
        }
        async fn authorize_result(
            &self,
            _: TransferKind,
            _: &Value,
            _: &Value,
            _: &AuditActor,
        ) -> Result<()> {
            unreachable!()
        }
        async fn context(
            &self,
            _: TransferKind,
            _: &Value,
            _: Option<&Value>,
            _: &AuditActor,
        ) -> Result<TransferContext> {
            unreachable!()
        }
        async fn recover(&self, _: TransferKind, _: &Value, _: &AuditActor) -> Result<Value> {
            unreachable!()
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin)
    }

    fn context() -> ContextInput {
        ContextInput { kind: TransferKind::SalesOrders, input: json!({"scope":"own"}), expires_at: 100 }
    }

    #[tokio::test]
    async fn protected_task_read_runs_only_after_qualification_on_original_snapshot() {
        let handler = Handler::default();
        let mut executor = SnapshotExecutor::default();
        let boundary = executor.boundary().unwrap();
        let calls = &handler.calls;
        let value = authorized_read(&handler, &context(), &actor(), &mut executor, |executor| {
            Box::pin(async move {
                assert!(boundary.is_current(executor));
                calls.lock().unwrap().push("read".into());
                Ok(42)
            })
        })
        .await
        .unwrap();

        assert_eq!(value, 42);
        assert_eq!(*handler.calls.lock().unwrap(), ["authorize", "read"]);
    }

    #[tokio::test]
    async fn revoked_qualification_never_starts_protected_read() {
        let handler = Handler { denied: true, ..Default::default() };
        let error = authorized_read(
            &handler,
            &context(),
            &actor(),
            &mut SnapshotExecutor::default(),
            |_| -> BoxFuture<'_, Result<()>> { panic!("拒绝资格后不得读取任务或私有文件") },
        )
        .await
        .unwrap_err();

        assert!(matches!(error, Error::Forbidden(reason) if reason == "业务资格已撤销"));
        assert_eq!(*handler.calls.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn non_snapshot_read_stops_before_business_qualification() {
        let handler = Handler::default();
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            let error =
                authorized_read(&handler, &context(), &actor(), executor, |_| -> BoxFuture<'_, Result<()>> {
                    panic!("非快照读取不得访问任务")
                })
                .await
                .unwrap_err();
            assert!(
                matches!(error, Error::ValidationError(reason) if reason == "任务读取授权必须使用快照事务")
            );
        }
        assert!(handler.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn delivery_rejects_any_changed_task_version() {
        let (job, _) = super::super::submit::test_batch();
        assert!(same_job(&job, &job).is_ok());
        let mut changed = job.clone();
        changed.base.version += 1;
        assert!(matches!(same_job(&job, &changed), Err(Error::ConflictError(_))));
    }

    #[test]
    fn delivery_rejects_other_task_even_with_same_version() {
        let (job, _) = super::super::submit::test_batch();
        let mut changed = job.clone();
        changed.base.id = "other-job".into();
        assert!(matches!(same_job(&job, &changed), Err(Error::ConflictError(_))));
    }
}
