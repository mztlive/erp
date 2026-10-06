//! 当前供应商申请与已授权内部审核的跨域读取入口。

use application_core::{AuditActor, PageView};
use erp_core::AccountKind;
use erp_identity::PortalActor;
use persistence_core::NoTransaction;
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
    /// 未知与其他供应商目标统一返回不存在。
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
    /// 未装配授权、身份错误或目标越权时拒绝。
    pub async fn admin_applications(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
    ) -> Result<PageView<Value>> {
        let authorization = self.internal_authorization(actor)?;
        if params.supplier_id.trim().is_empty()
            || !authorization.supplier_readable(actor, &params.supplier_id, &mut NoTransaction).await?
        {
            return Err(hidden_target());
        }
        let query = PortalQuery::new(&PortalListParams {
            q: params.q.clone(),
            status: params.status.clone(),
            page: params.page,
            page_size: params.page_size,
        })?;
        let ids = PortalApplicationRepository::new(&self.db)
            .application_ids(&params.supplier_id, &query, &mut NoTransaction)
            .await?;
        let mut allowed = Vec::new();
        for item in ids {
            if authorization.request_readable(actor, &item, &mut NoTransaction).await? {
                allowed.push(item);
            }
        }
        let (records, total) = PortalApplicationRepository::new(&self.db)
            .application_page(&params.supplier_id, &query, Some(&allowed), &mut NoTransaction)
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
    /// 未知或越权统一返回不存在，未注入授权时拒绝。
    pub async fn admin_application(&self, actor: &AuditActor, id: &str) -> Result<Value> {
        if !self.internal_authorization(actor)?.request_readable(actor, id, &mut NoTransaction).await? {
            return Err(hidden_target());
        }
        PortalApplicationRepository::new(&self.db).any_application(id, &mut NoTransaction).await?.internal()
    }

    /// 默认拒绝内部读取，也拒绝供应商伪装内部调用。
    pub(super) fn internal_authorization(
        &self,
        actor: &AuditActor,
    ) -> Result<&dyn PortalReadAuthorizationPort> {
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("仅内部人员可以读取审核资料".into()));
        }
        self.authorization.as_deref().ok_or_else(|| Error::Forbidden("申请读取授权未装配".into()))
    }
}

/// 不存在或范围外目标使用一致错误。
fn hidden_target() -> Error {
    Error::NotFound("申请不存在或无权查看".into())
}
