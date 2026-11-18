//! 应付往来子账列表、详情与创建编排。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::PayableAccountId;
use erp_finance::repository::PayableExt;
use erp_finance::service::payable::prepare_payable_account;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_party::SensitiveDataCodec;
use erp_permissions::payable;
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::finance::payable::PayableReadService;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::dto::{
    CreatePayableAccountRequest, PayableAccountView, PaymentRecipientRevealView,
    RevealPaymentRecipientRequest,
};
use super::{PayableProcess, payment_task};
use crate::{Error, Result};

impl PayableProcess {
    /// 拒绝旧应付直付入口的收款账号揭示，不读取或解密账户。
    ///
    /// # 参数
    /// * `_id` - 旧应付往来子账 ID。
    /// * `req` - 旧任务与页面所见收款账户，仍需校验请求和任务版本。
    /// * `_actor` - 当前操作人。
    /// * `_sensitive_data` - 敏感数据编解码器，本入口不使用。
    ///
    /// # 返回
    /// 不返回收款账号。
    ///
    /// # 错误
    /// 保留请求和任务版本的校验错误；校验通过后固定返回冲突。
    pub async fn reveal_payment_recipient(
        &self,
        _id: &str,
        req: RevealPaymentRecipientRequest,
        _actor: &AuditActor,
        _sensitive_data: &SensitiveDataCodec,
    ) -> Result<PaymentRecipientRevealView> {
        payment_task::reject_legacy_payment_reveal(&req)
    }
    /// 建立应付往来子账与原始应付分录（跨集合事务写入）。
    ///
    /// 校验来源单据存在（D15 `purchase_orders()`）；同事务写入子账与分录，
    /// 保证「子账 + 原始应付」原子可见（数据模型 §6.9）。业务幂等唯一
    /// `(payable_account_id, source_fact_type, source_document_id,
    /// source_revision_id, entry_type, source_sequence)` 由唯一索引保证。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源采购单不存在
    /// * `ConflictError` - 业务唯一键重复
    pub async fn create_payable_account(
        &self,
        req: CreatePayableAccountRequest,
        actor: &AuditActor,
    ) -> Result<PayableAccountView> {
        req.validate()?;
        let source_id = req.source_document_id.clone();
        let (account, entry) = prepare_payable_account(req, actor.id())?;
        let account_id = PayableAccountId::new(account.base.id.clone());
        let audit = actor.clone().resource_log(
            "payable_account.create",
            "payable_account",
            account_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let authorization = self.authorization_rbac.authorization.clone();
        let writer = actor.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&writer, &payable::ACCOUNT_CREATE, AccessTarget::Operation, executor)
                        .await?;
                    db.purchase_orders()
                        .find_by_id(&source_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
                    db.payable().create_payable_with_entry(&account, &entry, executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    created_account_view(
                        &PayableReadService::new(db.clone()),
                        &authorization,
                        &writer,
                        &account.base.id,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

/// 写后以原创建资格开启读取阶段，完整投影沿用原事务，不追加详情菜单资格。
async fn created_account_view(
    read: &PayableReadService,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<PayableAccountView> {
    authorization.require(actor, &payable::ACCOUNT_CREATE, AccessTarget::Operation, executor).await?;
    Ok(read.payable_account_view(id.to_owned(), true, executor).await?)
}

#[cfg(test)]
mod result_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 纯创建资格也必须在原快照检查；不可用资格不能回读供应商与账户数据。
    #[tokio::test]
    async fn created_account_reauthorizes_before_account_or_supplier_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("created_payable");
        let read = PayableReadService::new(db);
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        for (executor, snapshot) in [
            (&mut ReadExecutor::default() as &mut dyn Executor, false),
            (&mut SnapshotExecutor::default() as &mut dyn Executor, true),
        ] {
            let error =
                created_account_view(&read, &authorization, &actor, "account", executor).await.unwrap_err();
            if snapshot {
                assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
            } else {
                assert!(
                    matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务")
                );
            }
        }
    }
}
