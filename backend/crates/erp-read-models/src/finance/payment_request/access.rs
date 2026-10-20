//! 付款申请自身动作沿每张真实采购来源授权，部分可见不得交付整单。

use application_core::AuditActor;
use erp_finance::entity::payable::{PayableSourceType, PurchasePaymentRequest};
use erp_finance::repository::{PayableExt, PurchasePaymentRequestSnapshotExt};
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    /// 在时间线原读取事务内重验申请全部来源。
    ///
    /// # 参数
    /// `actor` 为认证账号，`request` 为申请，`executor` 为原事务。
    /// # 返回
    /// 当前详情范围覆盖全部来源时返回 true。
    /// # 错误
    /// 权限配置及数据库故障继续传播。
    pub async fn timeline_readable(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let scope = self.source_scope(actor, "detail", executor).await?;
        if !self.readable(request, &scope, executor).await? {
            return Ok(false);
        }
        for snapshot in
            self.db.purchase_payment_request_snapshots().find_for_request(&request.base.id, executor).await?
        {
            if !self.readable(&snapshot.request, &scope, executor).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// 在原命令事务证明申请动作和全部采购来源，不能由供应商可见性替代。
    ///
    /// # 参数
    /// * `actor` / `action` - 当前账号及申请资源动作。
    /// * `ids` / `executor` - 必须全部覆盖的采购身份与原事务。
    /// # 返回
    /// 返回全部当前授权采购对象。
    /// # 错误
    /// 任一采购缺失或不在来源范围内时整体拒绝。
    pub async fn require_request_sources(
        &self,
        actor: &AuditActor,
        action: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        if ids.is_empty() || ids.len() > 100 {
            return Err(Error::ValidationError("付款申请须包含 1 至 100 张采购单".into()));
        }
        let scope = self.source_scope(actor, action, executor).await?;
        let mut orders = Vec::with_capacity(ids.len());
        for id in ids {
            let order = self
                .db
                .purchase_orders()
                .find_authorized(id, &scope, executor)
                .await?
                .ok_or_else(|| Error::NotFound("付款申请来源不存在或无权处理".into()))?;
            orders.push(order);
        }
        Ok(orders)
    }

    pub(super) async fn source_scope(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseReadScope> {
        let (_, authorization) = self
            .funds_access()
            .resolve_with_purchase(
                actor,
                "purchase_payment_request",
                action,
                &self.purchase_access(),
                executor,
            )
            .await?;
        authorization.purchase_scope.ok_or_else(|| Error::Forbidden("付款申请采购来源范围不可用".into()))
    }

    pub(super) async fn readable(
        &self,
        request: &PurchasePaymentRequest,
        scope: &PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let ids = self.request_source_ids(request, executor).await?;
        if ids.is_empty() {
            return Ok(false);
        }
        for id in ids {
            if self.db.purchase_orders().find_authorized(&id, scope, executor).await?.is_none() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn request_source_ids(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if !request.lines.is_empty() {
            return Ok(request.lines.iter().map(|line| line.purchase_order_id.clone()).collect());
        }
        let mut ids = Vec::new();
        for line in &request.data.lines {
            let Some(account) =
                self.db.payable_accounts().find_by_id(&line.payable_account_id, executor).await?
            else {
                return Ok(Vec::new());
            };
            if account.source_type != PayableSourceType::PurchaseOrder {
                return Ok(Vec::new());
            }
            ids.push(account.source_document_id);
        }
        Ok(ids)
    }
}
