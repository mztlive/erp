//! CustomerRefund 详情与分页视图装配。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_returns::repository::ReturnsExt;
use erp_returns::repository::prelude::*;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::prelude::*;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ReturnsReadService;
use super::dto::{CustomerRefundListParams, CustomerRefundView, PageView, SortDir};
use crate::{Error, Result};
/// 客户退款列表筛选条件类型。
type CustomerRefundFilter = <mongodb::Database as ReturnsExt>::CustomerRefundFilter;
use erp_workflow::DocumentRegistryExt;

use super::customer_refund_list::{
    CustomerRefundListFacts, customer_refund_view_from_facts, map_customer_refund_list_page,
};

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 调用此入口时，分页查询客户退款列表。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号，按固定列表入口检查功能资格。
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// 参数校验或排序白名单失败时返回 `ValidationError`。退款或审批注册读取失败时返回对应错误。
    pub async fn customer_refund_list(
        &self,
        actor: &AuditActor,
        params: &CustomerRefundListParams,
    ) -> Result<PageView<CustomerRefundView>> {
        let this = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { this.customer_refund_list_snapshot(&actor, &params, executor).await })
            })
            .await
    }

    // 处理客户退款时，授权、完整过滤及结果投影共用调用方快照。
    async fn customer_refund_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &CustomerRefundListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<CustomerRefundView>> {
        self.authorization
            .require(actor, &returns::CUSTOMER_REFUND_CUSTOMER_REFUND_LIST, AccessTarget::Operation, executor)
            .await?;

        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerRefundFilter {
            refund_no: query.refund_no,
            customer_id: query.customer_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };

        let page = self.db.customer_refunds().search_customer_refunds(&filter, executor).await?;
        let document_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let documents = self.db.business_documents().find_documents_by_ids(&document_ids, executor).await?;

        let items = map_customer_refund_list_page(
            page.items.into_iter().map(CustomerRefundListFacts::from).collect(),
            documents,
        );
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 读取客户退款详情。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号，按固定详情入口检查功能资格。
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    ///
    /// 仓储或审批绑定读取失败时返回对应错误。
    pub async fn customer_refund_detail(&self, id: &str, actor: &AuditActor) -> Result<CustomerRefundView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.authorization
                        .require(
                            &actor,
                            &returns::CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;

                    this.load_customer_refund_view(&id, executor).await
                })
            })
            .await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 读取客户退款时，调用方必须先完成所属命令授权，并传入原执行器。
    /// 装配客户退款单视图。
    ///
    /// # 参数
    /// * `executor` - 已完成所属命令授权的原读取或事务执行器。
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    ///
    /// 仓储或审批绑定读取失败时返回对应错误。
    pub async fn load_customer_refund_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerRefundView> {
        Self::ensure_command_snapshot(executor)?;
        let id = id.to_owned();

        let refund = self
            .db
            .customer_refunds()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
        let binding = super::approval::optional_approval_binding(&self.db, &id, executor).await?;

        let mut view =
            customer_refund_view_from_facts(CustomerRefundListFacts::from_refund(&refund), binding.as_ref());
        view.approval = super::approval::load_runtime(
            &self.db,
            DocumentType::CustomerRefund,
            &id,
            view.approval,
            executor,
        )
        .await?;
        Ok(view)
    }
}
