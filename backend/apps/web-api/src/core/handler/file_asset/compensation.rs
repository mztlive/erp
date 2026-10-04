//! 对象存储上传的命令完成与补偿边界。
//!
//! 数据库确认未提交或幂等结果未消费本次上传时清理对象；提交结果未知时保留对象。

use std::future::Future;

use erp_processes::{Error, Result};
use erp_support::PendingFileAssetRequest;
use tracing::error;

use crate::app_state::AppState;

/// 根据数据库命令结果完成本次上传，保持原成功值及错误分类。
///
/// # 参数
/// * `state` - 提供对象存储客户端的应用状态
/// * `requests` - 仅属于本次请求的新上传对象
/// * `result` - 数据库命令结果；提交后的读取错误须放在成功载荷中，保留附件消费事实
/// * `assets_committed` - 成功结果是否消费了本次上传；普通成功固定返回 `true`
///
/// # 返回
/// 完成必要的对象补偿后，原样返回命令结果；调用方随后展开载荷中的详情读取结果。
///
/// # 错误
/// 保留原命令错误；提交结果未知时不删除对象。已确认提交后的读取错误不得转换为
/// 外层失败而触发补偿。清理失败只记日志，不覆盖命令结果或载荷中的读取错误。
pub(crate) async fn finish_asset_command<T>(
    state: &AppState,
    requests: &[PendingFileAssetRequest],
    result: Result<T>,
    assets_committed: impl FnOnce(&T) -> bool,
) -> Result<T> {
    finish_with_cleanup(result, assets_committed, || delete_pending_asset_objects(state, requests)).await
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

/// 删除本次尚未登记或已确定回滚的上传批次，作为对象存储补偿。
///
/// # 参数
/// * `state` - 提供对象存储客户端的应用状态
/// * `requests` - 仅属于本次请求、允许删除的对象
///
/// # 返回
/// 无；逐个尝试删除本批对象。
///
/// # 错误
/// 清理失败只记日志，继续清理剩余对象，不覆盖业务命令结果。
pub(crate) async fn delete_pending_asset_objects(state: &AppState, requests: &[PendingFileAssetRequest]) {
    for request in requests {
        if let Err(storage_error) = state.storage().delete(&request.registration.storage_object_key).await {
            error!(
                error = %storage_error,
                object_key = %request.registration.storage_object_key,
                "Failed to compensate unregistered file object"
            );
        }
    }
}

/// 判断数据库失败是否已确定没有提交，可以删除本次上传对象。
///
/// # 参数
/// * `error` - 遵循事务提交结果分类的命令错误
///
/// # 返回
/// 提交结果未知时返回 `false`；其余失败按事务合同允许补偿。
///
/// # 错误
/// 无。
pub(crate) fn should_compensate_pending_assets(error: &Error) -> bool {
    !matches!(error, Error::OutcomeUnknown(_))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use erp_processes::finance_posting::payable::SupplierPaymentWithAssetsResult;
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
            view: Err(Error::Internal("付款详情读取失败".to_string())),
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
        assert!(matches!(result.view, Err(Error::Internal(message)) if message == "付款详情读取失败"));
    }

    #[tokio::test]
    async fn replayed_payment_with_detail_failure_cleans_unused_upload_before_returning_read_error() {
        let cleanup_count = Cell::new(0);
        let payment = SupplierPaymentWithAssetsResult {
            view: Err(Error::NotFound("原付款详情暂不可读取".to_string())),
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
        assert!(matches!(result.view, Err(Error::NotFound(message)) if message == "原付款详情暂不可读取"));
    }

    #[tokio::test]
    async fn recovered_unknown_payment_with_detail_failure_keeps_upload_and_original_unknown_error() {
        let cleanup_count = Cell::new(0);
        let original_unknown =
            PersistenceError::CommitOutcomeUnknown(MongoError::custom("original unknown commit result"));
        let original_display = original_unknown.to_string();
        let payment = SupplierPaymentWithAssetsResult {
            view: Err(Error::OutcomeUnknown(original_unknown)),
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
        let Err(Error::OutcomeUnknown(source)) = result.view else {
            panic!("详情读取失败必须保留最初的提交未知错误");
        };
        assert_eq!(source.to_string(), original_display);
    }
}
