//! 选品读取在完整补齐后检查原查询凭证，再向事务入口交付结果。
use std::future::Future;
use std::pin::Pin;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_permissions::Permission;
use erp_sales::repository::sales_selection::SelectionReadScope;
use erp_sales::service::sales_selection::SelectionAccess;
use persistence_core::Executor;

use crate::Result;

type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// 读取编排只需签发和交付检查，不能追加对象授权或切换读取阶段。
#[async_trait]
pub(super) trait ReadAuthorization: Send + Sync {
    type Scope: Send + Sync;

    // 为固定入口取得当前读取凭证。
    //
    // actor/permission 为固定入口，executor 为原事务执行器。当前查询阶段凭证。功能资格、查询映射或依赖读取失败时拒绝。
    async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Self::Scope>;

    // 在业务读取和展示补齐后检查原凭证。
    //
    // scope 为原查询凭证，executor 为相同事务执行器。当前阶段及依赖均有效时成功。阶段变化或授权依赖到期时拒绝交付。
    async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl ReadAuthorization for SelectionAccess {
    type Scope = SelectionReadScope;

    async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Self::Scope> {
        Ok(self.resolve(actor, permission, executor).await?)
    }

    async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()> {
        Ok(scope.validate(executor).await?)
    }
}

// 在同一快照执行查询、完整业务读取与最终交付检查。
//
// authorization 为共享授权能力，actor/permission 为固定入口，executor 为原事务；read 完成根读取及全部补齐。
//
// 仅在最终交付检查成功后返回读取结果。查询、业务读取或交付检查失败时丢弃结果并保留错误分类。
pub(super) async fn checked<A, T, F>(
    authorization: &A,
    actor: &AuditActor,
    permission: &Permission,
    executor: &mut dyn Executor,
    read: F,
) -> Result<T>
where
    A: ReadAuthorization,
    F: for<'a> FnOnce(&'a A::Scope, &'a mut dyn Executor) -> ReadFuture<'a, T>,
{
    let scope = authorization.query(actor, permission, executor).await?;
    let result = read(&scope, executor).await?;
    authorization.validate(&scope, executor).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use erp_core::AccountKind;
    use erp_permissions::sales_selection;
    use persistence_core::ExecutionBoundary;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    struct Authorization {
        events: Mutex<Vec<&'static str>>,
        executor: Mutex<Option<usize>>,
        permission: Permission,
        expired: AtomicBool,
    }

    impl Authorization {
        // 记录真实编排访问的执行器，要求授权、读取和交付始终复用同一实例。
        fn observe(&self, event: &'static str, executor: &mut dyn Executor) {
            let pointer = executor as *mut dyn Executor as *mut () as usize;
            let mut expected = self.executor.lock().unwrap();
            assert_eq!(*expected.get_or_insert(pointer), pointer);
            self.events.lock().unwrap().push(event);
        }
    }

    #[async_trait]
    impl ReadAuthorization for Authorization {
        type Scope = ExecutionBoundary;

        async fn query(
            &self,
            _: &AuditActor,
            permission: &Permission,
            executor: &mut dyn Executor,
        ) -> Result<Self::Scope> {
            assert_eq!(permission, &self.permission);
            self.observe("query", executor);
            Ok(executor.boundary().unwrap())
        }

        async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()> {
            self.observe("validate", executor);
            if !scope.is_current(executor) {
                return Err(Error::ValidationError("读取阶段已变化".into()));
            }
            if self.expired.load(Ordering::SeqCst) {
                return Err(Error::ConflictError(
                    "DATA_SCOPE_CHANGED：选品资料的查看范围需要重新确认，请从第一页刷新".into(),
                ));
            }
            Ok(())
        }
    }

    // 册详情、链接、方案详情和附属读取共用同一真实交付编排。
    #[tokio::test]
    async fn delivery_follows_complete_read_and_uses_original_executor() {
        for permission in [
            sales_selection::BOOKLET_DETAIL,
            sales_selection::BOOKLET_COPY_LINK,
            sales_selection::PROPOSAL_DETAIL,
        ] {
            let authorization = Arc::new(Authorization {
                events: Mutex::new(vec![]),
                executor: Mutex::new(None),
                permission: permission.clone(),
                expired: AtomicBool::new(false),
            });
            let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
            let mut executor = SnapshotExecutor::default();
            let expected = &mut executor as *mut SnapshotExecutor as usize;
            let reader = authorization.clone();

            let result = checked(
                authorization.as_ref(),
                &actor,
                &permission,
                &mut executor,
                move |scope, executor| {
                    Box::pin(async move {
                        assert!(scope.is_current(executor));
                        reader.observe("root", executor);
                        reader.observe("display", executor);
                        Ok("complete-view")
                    })
                },
            )
            .await
            .unwrap();

            assert_eq!(result, "complete-view");
            assert_eq!(*authorization.executor.lock().unwrap(), Some(expected));
            assert_eq!(*authorization.events.lock().unwrap(), ["query", "root", "display", "validate"]);
        }
    }

    // 补齐期间到期或跨阶段时，已组装结果仍须丢弃。
    #[tokio::test]
    async fn changed_delivery_discards_the_completed_view() {
        for expire in [true, false] {
            let authorization = Arc::new(Authorization {
                events: Mutex::new(vec![]),
                executor: Mutex::new(None),
                permission: sales_selection::BOOKLET_DETAIL,
                expired: AtomicBool::new(false),
            });
            let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
            let reader = authorization.clone();

            let error = checked(
                authorization.as_ref(),
                &actor,
                &sales_selection::BOOKLET_DETAIL,
                &mut SnapshotExecutor::default(),
                move |_, executor| {
                    Box::pin(async move {
                        reader.observe("root", executor);
                        reader.observe("display", executor);
                        if expire {
                            reader.expired.store(true, Ordering::SeqCst);
                        } else {
                            executor.next_read();
                        }
                        Ok("unreleased-view")
                    })
                },
            )
            .await
            .unwrap_err();

            if expire {
                assert!(
                    matches!(error, Error::ConflictError(message) if message.starts_with("DATA_SCOPE_CHANGED"))
                );
            } else {
                assert!(matches!(error, Error::ValidationError(message) if message == "读取阶段已变化"));
            }
            assert_eq!(*authorization.events.lock().unwrap(), ["query", "root", "display", "validate"]);
        }
    }

    // 根读取失败时保留原错误，不执行后续补齐或交付检查。
    #[tokio::test]
    async fn failed_root_read_stops_before_delivery() {
        let authorization = Arc::new(Authorization {
            events: Mutex::new(vec![]),
            executor: Mutex::new(None),
            permission: sales_selection::PROPOSAL_DETAIL,
            expired: AtomicBool::new(false),
        });
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let reader = authorization.clone();

        let result: Result<String> = checked(
            authorization.as_ref(),
            &actor,
            &sales_selection::PROPOSAL_DETAIL,
            &mut SnapshotExecutor::default(),
            move |_, executor| {
                Box::pin(async move {
                    reader.observe("root", executor);
                    Err(Error::NotFound("销售方案不存在或无权查看".into()))
                })
            },
        )
        .await;

        assert!(matches!(result, Err(Error::NotFound(message)) if message == "销售方案不存在或无权查看"));
        assert_eq!(*authorization.events.lock().unwrap(), ["query", "root"]);
    }
}
