//! 发布回放先在同一快照中核验当前授权，再读取原请求结果。
use async_trait::async_trait;
use erp_permissions::sales_selection;
use persistence_core::{Executor, Transactional};

use super::SalesSelectionService;
use crate::Result;
use crate::dto::sales_selection::SalesSelectionBookletView;
use crate::entity::sales_selection::IdempotencyOperation;

impl SalesSelectionService {
    /// 回放结果和当前发布资格使用同一快照，拒绝后不读取历史业务数据。
    pub(super) async fn publish_replay(
        &self,
        booklet_id: &str,
        actor_id: &str,
        key: &str,
        hash: &str,
    ) -> Result<Option<SalesSelectionBookletView>> {
        let service = self.clone();
        let request = PublishReplay {
            booklet_id: booklet_id.to_owned(),
            actor_id: actor_id.to_owned(),
            key: key.to_owned(),
            hash: hash.to_owned(),
        };
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { replay(&service, &request, executor).await })
            })
            .await
    }
}

struct PublishReplay {
    booklet_id: String,
    actor_id: String,
    key: String,
    hash: String,
}

/// 限定为发布回放的两个步骤，生产与替身执行相同的授权先行编排。
#[async_trait]
trait ReplaySource: Sync {
    async fn authorize(&self, id: &str, executor: &mut dyn Executor) -> Result<()>;
    async fn receipt(
        &self,
        request: &PublishReplay,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSelectionBookletView>>;
}

#[async_trait]
impl ReplaySource for SalesSelectionService {
    async fn authorize(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.authorize_booklet(id, &sales_selection::BOOKLET_PUBLISH, executor).await
    }

    async fn receipt(
        &self,
        request: &PublishReplay,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSelectionBookletView>> {
        self.replay_idempotency(
            IdempotencyOperation::Publish,
            &request.actor_id,
            &request.key,
            &request.hash,
            executor,
        )
        .await
    }
}

/// 同键重试不能用原授权代替当前资格、范围和真实客户来源检查。
async fn replay(
    source: &impl ReplaySource,
    request: &PublishReplay,
    executor: &mut dyn Executor,
) -> Result<Option<SalesSelectionBookletView>> {
    source.authorize(&request.booklet_id, executor).await?;
    source.receipt(request, executor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;
    use crate::entity::sales_selection::{
        PoolFilterSnapshot, PoolSource, PoolSourceKind, SalesSelectionBooklet, SalesSelectionBookletData,
        SelectionForm, SubmitMode,
    };

    struct Source {
        allowed: bool,
        result: Option<SalesSelectionBookletView>,
        calls: Mutex<Vec<&'static str>>,
    }

    #[async_trait]
    impl ReplaySource for Source {
        async fn authorize(&self, id: &str, _: &mut dyn Executor) -> Result<()> {
            assert_eq!(id, "booklet");
            self.calls.lock().unwrap().push("authorize");
            if self.allowed {
                Ok(())
            } else {
                Err(Error::Forbidden("当前账号无权发布该选品册".into()))
            }
        }

        async fn receipt(
            &self,
            request: &PublishReplay,
            _: &mut dyn Executor,
        ) -> Result<Option<SalesSelectionBookletView>> {
            assert_eq!((&*request.actor_id, &*request.key, &*request.hash), ("actor", "key", "hash"));
            self.calls.lock().unwrap().push("receipt");
            Ok(self.result.clone())
        }
    }

    fn request() -> PublishReplay {
        PublishReplay {
            booklet_id: "booklet".into(),
            actor_id: "actor".into(),
            key: "key".into(),
            hash: "hash".into(),
        }
    }

    fn result() -> SalesSelectionBookletView {
        let booklet = SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("booklet"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("customer"),
                customer_no: "C001".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "actor".into(),
                business_org_unit_id: "org".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::ByQuantity,
                access_password_hash: Some("hash".into()),
                per_person_budget: None,
                voucher_count: None,
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .unwrap(),
                tiers: vec![],
                created_by: "actor".into(),
            },
        )
        .unwrap();
        SalesSelectionService::booklet_view(&booklet, &[], None, None)
    }

    #[tokio::test]
    async fn revoked_authorization_prevents_reading_an_existing_publication_receipt() {
        let source = Source { allowed: false, result: Some(result()), calls: Mutex::new(vec![]) };
        let error = replay(&source, &request(), &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::Forbidden(reason) if reason == "当前账号无权发布该选品册"));
        assert_eq!(*source.calls.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn authorized_publication_replays_the_original_result() {
        let source = Source { allowed: true, result: Some(result()), calls: Mutex::new(vec![]) };
        let view = replay(&source, &request(), &mut SnapshotExecutor::default()).await.unwrap().unwrap();
        assert_eq!(view.id, "booklet");
        assert_eq!(view.customer_id, "customer");
        assert_eq!(*source.calls.lock().unwrap(), ["authorize", "receipt"]);
    }

    #[tokio::test]
    async fn authorized_new_request_has_no_replay_result() {
        let source = Source { allowed: true, result: None, calls: Mutex::new(vec![]) };
        assert!(replay(&source, &request(), &mut SnapshotExecutor::default()).await.unwrap().is_none());
        assert_eq!(*source.calls.lock().unwrap(), ["authorize", "receipt"]);
    }
}
