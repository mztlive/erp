//! 当前供应商申请与已授权内部审核的跨域读取入口。

use application_core::{AuditActor, PageView};
use erp_identity::PortalActor;
use persistence_core::{Executor, NoTransaction, Transactional};
use serde_json::Value;

use super::repository::applications::PortalApplicationRepository;
use super::repository::query::PortalQuery;
use super::{
    PortalAdminListParams, PortalApplicationView, PortalListParams, PortalReadAuthorizationPort,
    SupplierPortalReadService,
};
use crate::{Error, Result};

impl SupplierPortalReadService {
    /// 混合读取当前供应商的报价、新品和合作条款申请。
    ///
    /// # 参数
    /// `actor` 是可信绑定；`params` 只收窄当前供应商。
    /// # 返回
    /// 按创建时间及身份稳定排序的允许列表分页。
    /// # 错误
    /// 供应商失效、输入非法或查询失败时拒绝。
    pub async fn applications(
        &self,
        actor: &PortalActor,
        params: &PortalListParams,
    ) -> Result<PageView<PortalApplicationView>> {
        self.portal_supplier(actor, &mut NoTransaction).await?;
        let query = PortalQuery::new(params)?;
        let (records, total) = PortalApplicationRepository::new(&self.db)
            .application_page(&actor.supplier_id, &query, None, &mut NoTransaction)
            .await?;
        let items = records.into_iter().map(|row| row.external()).collect::<Result<Vec<_>>>()?;
        Ok(PageView { items, total, page: query.page, page_size: query.page_size })
    }

    /// 在绑定供应商范围内精确读取任一申请。
    ///
    /// # 参数
    /// `actor` 是可信绑定；`id` 是请求的申请身份。
    /// # 返回
    /// 可见的原稿、提交、决定及实际结果。
    /// # 错误
    /// 未知与其他供应商目标统一返回 `NotFound`。供应商认证失效、查询或序列化失败时也会拒绝。
    pub async fn application(&self, actor: &PortalActor, id: &str) -> Result<PortalApplicationView> {
        self.portal_supplier(actor, &mut NoTransaction).await?;
        PortalApplicationRepository::new(&self.db)
            .scoped_application(&actor.supplier_id, id, &mut NoTransaction)
            .await?
            .external()
    }

    /// 内部读取指定供应商的申请并逐对象证明真实读取资格。
    ///
    /// # 参数
    /// `actor` 为内部账号；`params` 必须指定目标供应商。
    /// # 返回
    /// 仅包含可读申请的页与总数，不用供应商可见性替代供给范围。
    /// # 错误
    /// 非内部身份或未装配授权时返回 `Forbidden`。目标供应商为空或不可读时返回 `NotFound`。
    /// 分页非法时返回 `ValidationError`。查询或序列化失败时返回对应错误。
    pub async fn admin_applications(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
    ) -> Result<PageView<Value>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.admin_application_page(&actor, &params, executor).await })
            })
            .await
    }

    /// 在原快照内过滤全部可读申请，再读取分页与计数。
    async fn admin_application_page(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<Value>> {
        self.require_management_supplier(actor, &params.supplier_id, executor).await?;
        let authorization = self.internal_authorization()?;
        let query = PortalQuery::new(&PortalListParams {
            q: params.q.clone(),
            status: params.status.clone(),
            page: params.page,
            page_size: params.page_size,
        })?;
        let ids = PortalApplicationRepository::new(&self.db)
            .application_ids(&params.supplier_id, &query, executor)
            .await?;
        let mut allowed = Vec::new();
        for item in ids {
            if authorization.request_readable(actor, &item, executor).await? {
                allowed.push(item);
            }
        }
        let (records, total) = PortalApplicationRepository::new(&self.db)
            .application_page(&params.supplier_id, &query, Some(&allowed), executor)
            .await?;
        let items = records.into_iter().map(|row| row.internal()).collect::<Result<Vec<_>>>()?;
        Ok(PageView { items, total, page: query.page, page_size: query.page_size })
    }

    /// 内部精确申请读取，先执行任务及对象读取合同。
    ///
    /// # 参数
    /// `actor` 为真实内部账号；`id` 是精确申请身份。
    /// # 返回
    /// 有权审核的完整领域事实及统一协议字段。
    /// # 错误
    /// 未知或越权统一返回 `NotFound`。非内部身份或未注入授权时返回 `Forbidden`。
    /// 查询或序列化失败时返回对应错误。
    pub async fn admin_application(&self, actor: &AuditActor, id: &str) -> Result<Value> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.admin_application_detail(&actor, &id, executor).await })
            })
            .await
    }

    /// 精确申请授权与完整响应始终在同一个快照内读取。
    async fn admin_application_detail(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("内部门户读取必须使用快照事务".into()));
        }
        if !self.internal_authorization()?.request_readable(actor, id, executor).await? {
            return Err(hidden_target());
        }
        PortalApplicationRepository::new(&self.db).any_application(id, executor).await?.internal()
    }

    /// 取得组合层装配的正式对象授权端口。
    ///
    /// # 参数
    /// 无；调用对象授权时传入真实身份和原执行器。
    ///
    /// # 返回
    /// 已装配的授权端口；账号资格及数据范围由 identity 检查。
    ///
    /// # 错误
    /// 未装配授权时返回 `Forbidden`。
    pub(super) fn internal_authorization(&self) -> Result<&dyn PortalReadAuthorizationPort> {
        self.authorization.as_deref().ok_or_else(|| Error::Forbidden("申请读取授权未装配".into()))
    }
}

/// 不存在或范围外目标使用一致错误。
fn hidden_target() -> Error {
    Error::NotFound("申请不存在或无权查看".into())
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::PortalAdminListParams;
    use crate::Error;
    use crate::supplier_portal::testing::{Outcome, service};

    #[tokio::test]
    async fn internal_application_reads_authorize_in_the_original_snapshot_before_data_reads() {
        let (service, authorization, actor) = service(Outcome::Denied).await;
        let params = PortalAdminListParams {
            supplier_id: "supplier".into(),
            q: None,
            status: None,
            page: None,
            page_size: None,
        };
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;
        assert!(matches!(
            service.admin_application_page(&actor, &params, &mut executor).await,
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            service.admin_application_detail(&actor, "application", &mut executor).await,
            Err(Error::NotFound(message)) if message == "申请不存在或无权查看"
        ));
        assert_eq!(
            *authorization.calls.lock().unwrap(),
            vec![("supplier", "supplier".into(), expected), ("request", "application".into(), expected),]
        );
    }

    #[tokio::test]
    async fn internal_application_reads_reject_unscoped_execution_without_calling_authorization() {
        let (service, authorization, actor) = service(Outcome::Allowed).await;
        let params = PortalAdminListParams {
            supplier_id: "supplier".into(),
            q: None,
            status: None,
            page: None,
            page_size: None,
        };
        let mut executor = NoTransaction;
        assert!(matches!(
            service.admin_application_page(&actor, &params, &mut executor).await,
            Err(Error::ValidationError(message)) if message == "内部门户读取必须使用快照事务"
        ));
        assert!(matches!(
            service.admin_application_detail(&actor, "application", &mut executor).await,
            Err(Error::ValidationError(message)) if message == "内部门户读取必须使用快照事务"
        ));
        assert!(authorization.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn application_authorization_errors_are_not_hidden_as_missing_objects() {
        let (service, _, actor) = service(Outcome::Failed).await;
        let result =
            service.admin_application_detail(&actor, "application", &mut SnapshotExecutor::default()).await;
        assert!(matches!(
            result,
            Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED：授权依赖已变化"
        ));
    }
}
