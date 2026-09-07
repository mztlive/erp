//! 应付往来子账列表、详情与创建编排。

use erp_finance::repository::PayableExt;
use erp_procurement::repository::PurchaseOrderExt;

use erp_audit::AuditExt;

use erp_core::ids::{PartyBankAccountId, PayableAccountId};

use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::dto::{
    CreatePayableAccountRequest, PayableAccountView, PaymentRecipientRevealView,
    RevealPaymentRecipientRequest,
};
use super::mapping::resolve_current_payment_recipient;
use super::payment_task;
use super::PayableService;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_party::SensitiveDataCodec;
use services::{Error, Result};

impl PayableService {
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
            return Err(Error::ConflictError(
                "供应商收款账户已变化，请刷新付款任务并重新核对".to_string(),
            ));
        }
        let account_number = sensitive_data.decrypt(&recipient.account_number_ciphertext)?;
        let audit = actor.clone().resource_log(
            "party_bank_account.reveal_for_payment",
            "party_bank_account",
            recipient.base.id.clone(),
        )?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(PaymentRecipientRevealView {
            bank_account_id: recipient.base.id,
            account_number,
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
        self.db
            .purchase_orders()
            .find_by_id(&req.source_document_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;

        let (account, entry) = erp_finance::service::payable::prepare_payable_account(req, actor.id())?;
        let account_id = PayableAccountId::new(account.base.id.clone());
        let audit = actor.clone().resource_log(
            "payable_account.create",
            "payable_account",
            account_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.payable()
                        .create_payable_with_entry(&account, &entry, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), services::Error>(())
                })
            })
            .await?;

        self.read().payable_account_detail(&account_id).await
    }
}
