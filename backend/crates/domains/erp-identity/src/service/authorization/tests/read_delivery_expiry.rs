//! 详情和复制链接的查询凭证必须在交付时仍处于有效期内。
use erp_core::common::time::Instant;
use erp_permissions::{sales_inquiry, sales_selection};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{actor, complete};
use crate::Error;

// 沿真实查询和交付路径验证：有效凭证可交付，读取完成后到期则拒绝。
async fn require_current_delivery(executor: &mut dyn Executor) {
    let service = complete().build().unwrap();
    for permission in [
        sales_selection::BOOKLET_COPY_LINK,
        sales_selection::BOOKLET_DETAIL,
        sales_selection::PROPOSAL_DETAIL,
        sales_inquiry::QUERY_SALES_DETAIL,
    ] {
        let access = service.query(&actor(), &permission, executor).await.unwrap();
        access.ensure_current(executor).unwrap();
        service.validate(&access, executor).await.unwrap();

        let mut expired = service.query(&actor(), &permission, executor).await.unwrap();
        expired.ensure_current(executor).unwrap();
        expired.validity.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(
            service.validate(&expired, executor).await,
            Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")
        ));
    }
}

// 快照内的详情和复制链接也不能交付已到期的查询结果。
#[tokio::test]
async fn snapshot_detail_and_copy_link_queries_reject_expired_delivery() {
    require_current_delivery(&mut SnapshotExecutor::default()).await;
}

// 非事务的详情和复制链接同样在最终交付时检查到期。
#[tokio::test]
async fn non_transactional_detail_and_copy_link_queries_reject_expired_delivery() {
    require_current_delivery(&mut ReadExecutor::default()).await;
}
