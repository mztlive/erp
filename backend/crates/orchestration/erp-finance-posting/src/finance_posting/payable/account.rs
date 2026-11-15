//! 应付往来子账列表、详情与创建编排。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::{PartyBankAccountId, PayableAccountId};
use erp_finance::repository::PayableExt;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_party::SensitiveDataCodec;
use erp_permissions::payable;
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::finance::payable::PayableReadService;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::dto::{
    CreatePayableAccountRequest, PayableAccountView, PaymentRecipientRevealView,
    RevealPaymentRecipientRequest,
};
use super::mapping::resolve_current_payment_recipient;
use super::{PayableProcess, payment_task};
use crate::{Error, Result};

impl PayableProcess {
    /// 在付款任务责任校验后揭示当前默认收款账号。
    ///
    /// 查看不修改任务版本；页面可在核对账号后继续使用同一任务版本提交付款。
    /// 每次成功揭示均写入敏感信息审计。
    ///
    /// # 参数
    /// * `id` - 当前付款任务绑定的应付往来子账 ID
    /// * `req` - 任务身份、任务版本与页面所见收款账户
    /// * `actor` - 当前操作人
    /// * `sensitive_data` - 应用启动期共享的敏感数据编解码器
    ///
    /// # 返回
    /// 返回完整收款账号和对应账户事实行主键。
    ///
    /// # 错误
    /// 任务责任、版本、账户身份或敏感密文不合法时失败关闭。
    pub async fn reveal_payment_recipient(
        &self,
        id: &str,
        req: RevealPaymentRecipientRequest,
        actor: &AuditActor,
        sensitive_data: &SensitiveDataCodec,
    ) -> Result<PaymentRecipientRevealView> {
        req.validate()?;
        let expected_task_version =
            erp_workflow::service::work_item::expected_task_version(&req.expected_task_version)?;
        let account_id = PayableAccountId::new(id);
        let (_, account) = payment_task::authorize_payment_execution(
            &self.db,
            &self.authorization_rbac,
            &req.work_item_id,
            expected_task_version,
            Some(&account_id),
            actor,
            &mut NoTransaction,
        )
        .await?;
        let recipient =
            resolve_current_payment_recipient(&self.db, &account.supplier_id, &mut NoTransaction).await?;
        if !recipient.matches_expected(
            &PartyBankAccountId::new(req.expected_bank_account_id.trim()),
            req.expected_bank_account_version,
        ) {
            return Err(Error::ConflictError("供应商收款账户已变化，请刷新付款任务并重新核对".to_string()));
        }
        let account_number = sensitive_data.decrypt(&recipient.account_number_ciphertext)?;
        let audit = actor.clone().resource_log(
            "party_bank_account.reveal_for_payment",
            "party_bank_account",
            recipient.base.id.clone(),
        )?;
        persist_log(&self.db, &audit, &mut NoTransaction).await?;
        Ok(PaymentRecipientRevealView {
            bank_account_id: recipient.base.id,
            account_number,
            account_name: recipient.account_name,
            bank_name: recipient.bank_name,
            bank_branch_name: recipient.bank_branch_name,
            bank_routing_number: recipient.bank_routing_number,
        })
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
        let (account, entry) = erp_finance::service::payable::prepare_payable_account(req, actor.id())?;
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
