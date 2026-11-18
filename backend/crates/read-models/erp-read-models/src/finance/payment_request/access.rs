//! 付款申请自身动作沿每张真实采购来源授权，部分可见不得交付整单。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_finance::entity::payable::{PayableSourceType, PurchasePaymentRequest};
use erp_finance::repository::{PayableExt, PurchasePaymentRequestSnapshotExt};
use erp_identity::CompiledAccess;
use erp_permissions::{Permission as PaymentPermission, purchase_payment_request as payment_permissions};
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
use crate::finance::funds_scope::{FundsAuthorization, SourceQuery};
use crate::{Error, Result};

pub(super) struct PaymentReadScope {
    scope: SourceQuery,
    authorization: FundsAuthorization,
}
impl PaymentReadScope {
    pub(super) fn root_access(&self) -> Result<&CompiledAccess> {
        self.authorization.root_access()
    }
    pub(super) async fn deliver(&self, executor: &mut dyn Executor) -> Result<()> {
        self.authorization.deliver(executor).await
    }
}

impl PurchasePaymentRequestReadService {
    /// 组装采购付款申请时，在时间线原读取事务内重验申请全部来源。
    ///
    /// # 参数
    /// `actor` 为认证账号，`request` 为申请，`executor` 为原事务。
    ///
    /// # 返回
    /// 当前详情范围覆盖全部来源时返回 true。
    ///
    /// # 错误
    /// 权限配置及数据库故障继续传播。
    pub async fn timeline_readable(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let scope = self.source_scope(actor, &payment_permissions::DETAIL, executor).await?;
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

        scope.deliver(executor).await?;
        Ok(true)
    }

    pub(super) async fn source_scope(
        &self,
        actor: &AuditActor,
        permission: &PaymentPermission,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReadScope> {
        let (_, authorization) =
            self.funds_access().resolve_with_purchase(actor, permission, executor).await?;
        let scope = authorization
            .purchase_scope
            .clone()
            .ok_or_else(|| Error::ValidationError("付款申请缺少采购来源".into()))?;
        Ok(PaymentReadScope { scope, authorization })
    }

    pub(super) async fn readable(
        &self,
        request: &PurchasePaymentRequest,
        scope: &PaymentReadScope,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let ids = self.request_source_ids(request, executor).await?;
        if ids.is_empty() {
            return Ok(false);
        }
        let allowed = scope.scope.ids_in(&self.db, &ids, executor).await?.into_iter().collect::<HashSet<_>>();
        Ok(ids.iter().all(|id| allowed.contains(id)))
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
