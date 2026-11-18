//! 内部开通与定向开放列表的安全视图，始终重验供应商范围。

use application_core::{AuditActor, PageView};
use erp_identity::PortalAccountView;
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::repository::management::{accounts_page, grants_page};
use super::repository::query::PortalQuery;
use super::{PortalAdminListParams, PortalListParams, SupplierPortalReadService};
use crate::{Error, Result};

/// 内部查看的定向开放关系，不包含公司销售价格或任何供应商报价。
#[derive(Debug, Serialize, Deserialize)]
pub struct PortalGrantView {
    pub id: String,
    pub supplier_id: String,
    pub sku_id: String,
    pub active: bool,
    pub version: u64,
    pub sku_no: Option<String>,
    pub name: Option<String>,
    pub specification: Option<String>,
}

impl SupplierPortalReadService {
    /// 查看已授权供应商的实名门户账号，过滤哈希及内部身份事实。
    ///
    /// # 参数
    /// `actor` 为内部账号；`params` 指定供应商及有界分页、账号名搜索和启停状态。
    /// # 返回
    /// 包含固定门户岗位、启停及会话版本的安全分页视图。
    /// # 错误
    /// 内部资格、供应商范围、筛选或查询失败时拒绝。
    pub async fn accounts(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
    ) -> Result<PageView<PortalAccountView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.account_page(&actor, &params, executor).await })
            })
            .await
    }

    /// 同一快照完成供应商资格、账号分页与计数。
    async fn account_page(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<PortalAccountView>> {
        self.require_management_supplier(actor, &params.supplier_id, executor).await?;
        let (query, active) = management_query(params)?;
        accounts_page(&self.db, &params.supplier_id, &query, active, executor).await
    }

    /// 查看已授权供应商的定向 SKU 开放关系及必要商品资料。
    ///
    /// # 参数
    /// `actor` 为内部账号；`params` 指定供应商及有界分页、商品搜索和启停状态。
    /// # 返回
    /// 当前及已撤销开放关系的分页，不包含其他报价或销售价。
    /// # 错误
    /// 内部资格、供应商范围、筛选或查询失败时拒绝。
    pub async fn grants(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
    ) -> Result<PageView<PortalGrantView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.grant_page(&actor, &params, executor).await })
            })
            .await
    }

    /// 同一快照完成供应商资格、开放关系及商品资料分页。
    async fn grant_page(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<PortalGrantView>> {
        self.require_management_supplier(actor, &params.supplier_id, executor).await?;
        let (query, active) = management_query(params)?;
        grants_page(&self.db, &params.supplier_id, &query, active, executor).await
    }

    /// 所有内部列表读取先在原快照中证明正式供应商详情资格。
    /// # 参数
    /// actor 为内部账号，supplier_id 为精确供应商，executor 为覆盖完整读取的原事务。
    /// # 返回
    /// 供应商可读时成功，不签发跨阶段凭证。
    /// # 错误
    /// 非快照执行器、缺失供应商或范围外拒绝；授权取证故障原样传播。
    pub(super) async fn require_management_supplier(
        &self,
        actor: &AuditActor,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("内部门户读取必须使用快照事务".into()));
        }

        if supplier_id.trim().is_empty()
            || !self.internal_authorization()?.supplier_readable(actor, supplier_id, executor).await?
        {
            return Err(Error::NotFound("供应商不存在或无权查看".into()));
        }
        Ok(())
    }
}

/// 统一分页与字面量搜索，管理状态只支持明确启停。
fn management_query(params: &PortalAdminListParams) -> Result<(PortalQuery, Option<bool>)> {
    let query = PortalQuery::new(&PortalListParams {
        q: params.q.clone(),
        status: None,
        page: params.page,
        page_size: params.page_size,
    })?;
    let active = match params
        .status
        .as_deref()
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        None => None,
        Some("ACTIVE") => Some(true),
        Some("INACTIVE") => Some(false),
        Some(_) => return Err(Error::ValidationError("启停状态须为 ACTIVE 或 INACTIVE".into())),
    };
    Ok((query, active))
}

#[cfg(test)]
mod tests {
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::{PortalAdminListParams, management_query};
    use crate::Error;
    use crate::supplier_portal::testing::{Outcome, service};

    #[tokio::test]
    async fn supplier_authorization_accepts_snapshot_and_propagates_denial_or_drift() {
        for outcome in [Outcome::Allowed, Outcome::Denied, Outcome::Failed] {
            let (service, authorization, actor) = service(outcome).await;
            let mut executor = SnapshotExecutor::default();
            let expected = &mut executor as *mut SnapshotExecutor as usize;
            let result = service.require_management_supplier(&actor, "supplier", &mut executor).await;
            match outcome {
                Outcome::Allowed => result.unwrap(),
                Outcome::Denied => assert!(matches!(result, Err(Error::NotFound(_)))),
                Outcome::Failed => assert!(matches!(
                    result,
                    Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED：授权依赖已变化"
                )),
            }
            assert_eq!(*authorization.calls.lock().unwrap(), vec![("supplier", "supplier".into(), expected)]);
        }
    }

    #[tokio::test]
    async fn management_pages_stop_before_reading_rows_when_supplier_is_denied() {
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
            service.account_page(&actor, &params, &mut executor).await,
            Err(Error::NotFound(_))
        ));
        assert!(matches!(service.grant_page(&actor, &params, &mut executor).await, Err(Error::NotFound(_))));
        assert_eq!(
            *authorization.calls.lock().unwrap(),
            vec![("supplier", "supplier".into(), expected), ("supplier", "supplier".into(), expected),]
        );

        let mut unscoped = ReadExecutor::default();
        assert!(matches!(
            service.account_page(&actor, &params, &mut unscoped).await,
            Err(Error::ValidationError(message)) if message == "内部门户读取必须使用快照事务"
        ));
        assert!(matches!(
            service.grant_page(&actor, &params, &mut unscoped).await,
            Err(Error::ValidationError(message)) if message == "内部门户读取必须使用快照事务"
        ));
        assert_eq!(authorization.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn management_status_has_no_application_or_internal_identity_aliases() {
        let mut params = PortalAdminListParams {
            supplier_id: "supplier1".into(),
            q: Some("  名称  ".into()),
            status: Some("inactive".into()),
            page: Some(2),
            page_size: Some(20),
        };
        let (query, active) = management_query(&params).unwrap();
        assert_eq!((query.page, query.skip, query.q.as_deref(), active), (2, 20, Some("名称"), Some(false)));

        params.status = Some("SUSPENDED".into());
        assert!(management_query(&params).is_err());
    }
}
