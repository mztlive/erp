//! 对象存储上传的命令完成与补偿边界。
//!
//! 数据库确认未提交或幂等结果未消费本次上传时清理对象；提交结果未知时保留对象。

use std::future::Future;

use erp_audit::execution::AuditFailure;
use erp_support::PendingFileAssetRequest;
use tracing::error;

use crate::http::state::HttpState;
use crate::wiring::errors::{Error, Result};

// 完成带附件的命令时，只清理由本次请求上传且已知未消费的对象。成功回放未使用新对象或事务确认失败时清理；结果未知时保留。
// 已确认写入后的详情读取故障须留在成功载荷中，不能触发补偿。清理故障仅记录，不覆盖原命令或详情读取结果。
pub(crate) async fn finish_asset_command<T, E: Into<Error>>(
    state: &HttpState,
    requests: &[PendingFileAssetRequest],
    result: std::result::Result<T, E>,
    assets_committed: impl FnOnce(&T) -> bool,
) -> Result<T> {
    finish_with_cleanup(result.map_err(Into::into), assets_committed, || {
        delete_pending_asset_objects(state, requests)
    })
    .await
}

async fn finish_with_cleanup<T, F, Fut>(
    result: Result<T>,
    assets_committed: impl FnOnce(&T) -> bool,
    cleanup: F,
) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let should_delete = match &result {
        Ok(value) => !assets_committed(value),
        Err(error) => should_compensate_pending_assets(error),
    };
    if should_delete {
        cleanup().await;
    }
    result
}

// 补偿时仅删除本次未登记或已确认回滚的上传对象，逐个尝试；清理失败只写日志，不改变业务命令结果。
pub(crate) async fn delete_pending_asset_objects(state: &HttpState, requests: &[PendingFileAssetRequest]) {
    for request in requests {
        if let Err(storage_error) =
            state.application().storage().delete(&request.registration.storage_object_key).await
        {
            error!(
                error = %storage_error,
                object_key = %request.registration.storage_object_key,
                "Failed to compensate unregistered file object"
            );
        }
    }
}

// 在命令失败后根据提交结果分类决定能否清理新上传对象；提交结果未知时不得补偿。
pub(crate) fn should_compensate_pending_assets(error: &impl AuditFailure) -> bool {
    !error.outcome_unknown()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use erp_finance_posting::Error as FinanceError;
    use erp_finance_posting::finance_posting::payable::SupplierPaymentWithAssetsResult;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::{Error, finish_with_cleanup};

    #[tokio::test]
    async fn committed_success_keeps_upload_and_original_result() {
        let cleanup_count = Cell::new(0);
        let result = finish_with_cleanup(
            Ok("created-id"),
            |_| true,
            || async {
                cleanup_count.set(cleanup_count.get() + 1);
            },
        )
        .await;

        assert_eq!(result.expect("committed result"), "created-id");
        assert_eq!(cleanup_count.get(), 0);
    }

    #[tokio::test]
    async fn replay_without_asset_commit_cleans_upload_and_keeps_original_result() {
        let cleanup_count = Cell::new(0);
        let result = finish_with_cleanup(
            Ok(("original-id", false)),
            |result| result.1,
            || async {
                cleanup_count.set(cleanup_count.get() + 1);
            },
        )
        .await;

        assert_eq!(result.expect("replayed result"), ("original-id", false));
        assert_eq!(cleanup_count.get(), 1);
    }

    #[tokio::test]
    async fn definite_failure_cleans_upload_and_preserves_error() {
        let cleanup_count = Cell::new(0);
        let result = finish_with_cleanup::<(), _, _>(
            Err(Error::ConflictError("stale-version".to_string())),
            |_| panic!("失败结果不读取成功值"),
            || async { cleanup_count.set(cleanup_count.get() + 1) },
        )
        .await;

        assert!(matches!(result, Err(Error::ConflictError(message)) if message == "stale-version"));
        assert_eq!(cleanup_count.get(), 1);
    }

    #[tokio::test]
    async fn unknown_commit_outcome_keeps_upload_and_preserves_error() {
        let cleanup_count = Cell::new(0);
        let error = Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
            "unknown commit result",
        )));
        let result = finish_with_cleanup::<(), _, _>(
            Err(error),
            |_| panic!("失败结果不读取成功值"),
            || async { cleanup_count.set(cleanup_count.get() + 1) },
        )
        .await;

        assert!(matches!(result, Err(Error::OutcomeUnknown(_))));
        assert_eq!(cleanup_count.get(), 0);
    }

    #[tokio::test]
    async fn committed_payment_with_detail_failure_keeps_upload_and_original_read_error() {
        let cleanup_count = Cell::new(0);
        let payment = SupplierPaymentWithAssetsResult {
            view: Err(FinanceError::Internal("付款详情读取失败".to_string())),
            assets_committed: true,
        };
        let result = finish_with_cleanup(
            Ok(payment),
            |payment| payment.assets_committed,
            || async { cleanup_count.set(cleanup_count.get() + 1) },
        )
        .await
        .expect("付款提交事实保持成功");

        assert_eq!(cleanup_count.get(), 0);
        assert!(result.assets_committed);
        assert!(matches!(result.view, Err(FinanceError::Internal(message)) if message == "付款详情读取失败"));
    }

    #[tokio::test]
    async fn replayed_payment_with_detail_failure_cleans_unused_upload_before_returning_read_error() {
        let cleanup_count = Cell::new(0);
        let payment = SupplierPaymentWithAssetsResult {
            view: Err(FinanceError::NotFound("原付款详情暂不可读取".to_string())),
            assets_committed: false,
        };
        let result = finish_with_cleanup(
            Ok(payment),
            |payment| payment.assets_committed,
            || async { cleanup_count.set(cleanup_count.get() + 1) },
        )
        .await
        .expect("重放回执保持成功");

        assert_eq!(cleanup_count.get(), 1);
        assert!(!result.assets_committed);
        assert!(
            matches!(result.view, Err(FinanceError::NotFound(message)) if message == "原付款详情暂不可读取")
        );
    }

    #[tokio::test]
    async fn recovered_unknown_payment_with_detail_failure_keeps_upload_and_original_unknown_error() {
        let cleanup_count = Cell::new(0);
        let original_unknown =
            PersistenceError::CommitOutcomeUnknown(MongoError::custom("original unknown commit result"));
        let original_display = original_unknown.to_string();
        let payment = SupplierPaymentWithAssetsResult {
            view: Err(FinanceError::OutcomeUnknown(original_unknown)),
            assets_committed: true,
        };
        let result = finish_with_cleanup(
            Ok(payment),
            |payment| payment.assets_committed,
            || async { cleanup_count.set(cleanup_count.get() + 1) },
        )
        .await
        .expect("已找到付款回执，保留附件可能已提交的事实");

        assert_eq!(cleanup_count.get(), 0);
        assert!(result.assets_committed);
        let Err(FinanceError::OutcomeUnknown(source)) = result.view else {
            panic!("详情读取失败必须保留最初的提交未知错误");
        };
        assert_eq!(source.to_string(), original_display);
    }
}
