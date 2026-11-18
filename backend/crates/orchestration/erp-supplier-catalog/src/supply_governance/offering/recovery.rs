//! 只有提交可能已成功的错误才恢复；恢复读取在新快照重新授权。
use std::future::Future;

use application_core::AuditActor;
use erp_supply::dto::supplier_offering::{
    CreateSupplierOfferingRequest, CreateSupplierOfferingResult, ReviseSupplierOfferingRequest,
    ReviseSupplierOfferingResult, UpdateSupplierOfferingAvailabilityRequest,
    UpdateSupplierOfferingAvailabilityResult,
};
use persistence_core::Transactional;

use super::SupplierOfferingProcess;
use crate::Result;

/// 确定失败保持原错误；未知结果只尝试一次已授权回执读取，不重试写入。
pub(super) async fn resolve<T, F, R>(transaction: Result<T>, read: F) -> Result<T>
where
    F: FnOnce() -> R,
    R: Future<Output = Result<Option<T>>>,
{
    match transaction {
        Ok(result) => Ok(result),
        Err(error) if error.command_may_have_committed() => read().await?.ok_or(error),
        Err(error) => Err(error),
    }
}

impl SupplierOfferingProcess {
    /// 创建回执与当前创建资格、真实供给读取使用新快照。
    /// # 参数
    /// 原目标、请求与当前身份。
    /// # 返回
    /// 当前授权通过的原回执；没有回执时返回 None。
    /// # 错误
    /// 当前授权、回执指纹、事务或读取失败时拒绝。
    pub(super) async fn created_result(
        &self,
        request: CreateSupplierOfferingRequest,
        actor: AuditActor,
    ) -> Result<Option<CreateSupplierOfferingResult>> {
        let domain = self.domain();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { Ok(domain.created_result(&request, &actor, executor).await?) })
            })
            .await
    }

    /// 修订回执与当前对象授权使用新快照。
    /// # 参数
    /// 原目标、请求与当前身份。
    /// # 返回
    /// 当前授权通过的原回执；没有回执时返回 None。
    /// # 错误
    /// 当前授权、回执指纹、事务或读取失败时拒绝。
    pub(super) async fn revised_result(
        &self,
        id: String,
        request: ReviseSupplierOfferingRequest,
        actor: AuditActor,
    ) -> Result<Option<ReviseSupplierOfferingResult>> {
        let domain = self.domain();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { Ok(domain.revised_result(&id, &request, &actor, executor).await?) })
            })
            .await
    }

    /// 可供回执与完整可供更新授权使用新快照。
    /// # 参数
    /// 原目标、请求与当前身份。
    /// # 返回
    /// 当前授权通过的原回执；没有回执时返回 None。
    /// # 错误
    /// 当前授权、回执指纹、事务或读取失败时拒绝。
    pub(super) async fn availability_result(
        &self,
        id: String,
        request: UpdateSupplierOfferingAvailabilityRequest,
        actor: AuditActor,
    ) -> Result<Option<UpdateSupplierOfferingAvailabilityResult>> {
        let domain = self.domain();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { Ok(domain.availability_result(&id, &request, &actor, executor).await?) },
                )
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;
    use crate::Error;

    /// 已成功及确定失败均不得读取旧回执覆盖原结果。
    #[tokio::test]
    async fn successful_and_definite_failures_do_not_recover_receipts() {
        let calls = AtomicUsize::new(0);
        let value = resolve(Ok(9), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(73))
        })
        .await;
        assert_eq!(value.unwrap(), 9);

        for error in [
            Error::Forbidden("revoked".into()),
            Error::Unauthenticated("disabled".into()),
            Error::ValidationError("invalid".into()),
            Error::BusinessLogicError("state".into()),
            Error::ConflictError("business version".into()),
            Error::Internal("evidence unavailable".into()),
        ] {
            let expected = error.to_string();
            let result = resolve(Err(error), || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Some(73))
            })
            .await;
            assert_eq!(result.unwrap_err().to_string(), expected);
        }

        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    /// 已确认可恢复的失败必须继续接受新快照中的授权拒绝。
    #[tokio::test]
    async fn unknown_commit_requires_current_authorization_and_preserves_missing_receipt() {
        let original =
            || Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("unknown")));
        let result: Result<u64> =
            resolve(Err(original()), || async { Err(Error::Forbidden("revoked".into())) }).await;
        assert!(matches!(result, Err(Error::Forbidden(reason)) if reason == "revoked"));

        let result: Result<u64> = resolve(Err(original()), || async { Ok(None) }).await;
        assert!(matches!(result, Err(Error::OutcomeUnknown(_))));

        let result = resolve(Err(original()), || async { Ok(Some(73)) }).await;
        assert_eq!(result.unwrap(), 73);
    }
}
