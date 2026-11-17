//! 审计展示读取：功能资格、分页和计数使用同一快照。
use std::sync::Arc;

use application_core::{AuditActor, Page};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::admin;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use crate::dto::{AuditLogItem, AuditLogListParams};
use crate::error::Result;
use crate::repository::{AuditExt, AuditLogFilter, AuditLogRepositoryExt};

/// 必须经过统一授权的审计日志展示入口。
#[derive(Clone)]
pub struct AuditLogReadService {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl AuditLogReadService {
    /// 创建审计读取服务。
    /// # 参数
    /// db 为审计数据库，authorization 为进程共享的完整授权服务。
    /// # 返回
    /// 绑定同一数据库及授权设施的服务。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 在快照事务中检查读取资格并查询分页审计日志。
    /// # 参数
    /// params 为列表条件，actor 为已认证的当前账号。
    /// # 返回
    /// 经授权、输入校验和文本归一化后的展示分页，计数使用同一执行器。
    /// # 错误
    /// 身份无效、缺少 audit_log:list、参数非法或数据库失败时返回对应错误。
    pub async fn audit_log_list(
        &self,
        params: &AuditLogListParams,
        actor: &AuditActor,
    ) -> Result<Page<AuditLogItem>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.page(&params, &actor, executor).await })
            })
            .await
    }

    /// 授权及查询沿用当前快照，拒绝时不读取审计记录。
    async fn page(
        &self,
        params: &AuditLogListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Page<AuditLogItem>> {
        let filter = self.filter(params, actor, executor).await?;
        let page = self.db.audit_logs().search_logs(&filter, executor).await?;
        Ok(Page::new(page.items.into_iter().map(Into::into).collect(), page.total))
    }

    /// 成功取证后构造仓储条件，避免无资格账号探测查询输入。
    async fn filter(
        &self,
        params: &AuditLogListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AuditLogFilter> {
        self.authorization
            .require(actor, &admin::AUDIT_LOG_LIST_AUDIT_LOGS, AccessTarget::Operation, executor)
            .await?;
        params.validate()?;
        Ok(AuditLogFilter::from(params))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    async fn service(authorization: Arc<AuthorizationService>) -> AuditLogReadService {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("audit_authorization_unit");
        AuditLogReadService::new(db, authorization)
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    fn authorization(granted: bool) -> Arc<AuthorizationService> {
        let permissions = if granted { vec![admin::AUDIT_LOG_LIST_AUDIT_LOGS] } else { vec![] };
        fixed_authorization(
            &permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap()
    }

    #[tokio::test]
    async fn allowed_read_prepares_normalized_filter_in_the_same_snapshot() {
        let service = service(authorization(true)).await;
        let params = AuditLogListParams {
            actor_account: Some("  admin01  ".into()),
            action: Some("   ".into()),
            page: Some(2),
            page_size: Some(7),
            ..Default::default()
        };
        let filter = service.filter(&params, &actor(), &mut SnapshotExecutor::default()).await.unwrap();
        assert_eq!(filter.actor_account.as_deref(), Some("admin01"));
        assert_eq!(filter.action, None);
        assert_eq!(filter.page, 2);
        assert_eq!(filter.page_size, 7);
    }

    #[tokio::test]
    async fn denied_or_unavailable_identity_stops_before_audit_storage() {
        let denied = service(authorization(false)).await;
        let invalid = AuditLogListParams { page: Some(0), ..Default::default() };
        let error = denied.page(&invalid, &actor(), &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::Forbidden(_)));

        let unavailable = service(unavailable_authorization()).await;
        let error = unavailable
            .page(&AuditLogListParams::default(), &actor(), &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn authorized_invalid_page_stops_before_audit_storage() {
        let service = service(authorization(true)).await;
        let params = AuditLogListParams { page_size: Some(101), ..Default::default() };
        let error = service.page(&params, &actor(), &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
    }
}
