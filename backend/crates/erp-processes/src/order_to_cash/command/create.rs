use application_core::{AuditActor, CommandFingerprint};
use erp_contract::{ContractExt, ContractStatus};
use erp_core::ids::{ContractId, CustomerAccountId, PartyId};
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::{
    CreateSalesOrderRequest, SalesOrderCreateIntent, SalesOrderDraftRequest, SalesOrderEditableDraftRequest,
};
use erp_sales::entity::command_receipt::SalesCommandResult;
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::command::identity::{
    sales_order_create_audit_id, sales_order_create_fingerprint,
};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::authorization::SalesCommandAccess;
use crate::order_to_cash::command_event::{SalesCommandEvent, finish_receipt_recovery, load_command_receipt};
use crate::{Error, Result};

mod persist;
mod submission;

use persist::persist_creation;

/// 同一认证建单请求的完整身份，供事前重放和失败查证共用。
struct CreationIdentity {
    idempotency_key: String,
    audit_id: String,
    fingerprint: String,
    key_hash: CommandFingerprint,
}

impl SalesOrderCommandProcess {
    /// 解析销售命令所选合同的客户身份，供 HTTP 层执行客户数据范围校验。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `contract_id` - 前端选择的合同稳定身份
    /// * `customer_id` - 无合同时前端选择的客户身份
    ///
    /// # 返回
    /// 返回所选合同所属客户或无合同命令所选客户的稳定身份。
    ///
    /// # 错误
    /// 合同或客户不存在、不可见时返回 `NotFound`；无合同且未选择客户时返回校验错误。
    ///
    /// # 关键业务约束
    /// 本入口只做事前检查；写入事务仍须 `related` 重验，不得当作唯一凭证。
    #[tracing::instrument(
        name = "sales_order.resolve_customer_scope",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "resolve_customer_scope")
    )]
    pub async fn sales_command_customer_id(
        &self,
        actor: &AuditActor,
        contract_id: &Option<ContractId>,
        customer_id: &Option<CustomerAccountId>,
    ) -> Result<CustomerAccountId> {
        let access = self.command_access(actor, "detail")?;
        if let Some(contract_id) = contract_id {
            let contract = access.load_contract(contract_id.as_ref(), &mut NoTransaction).await?;
            return Ok(contract.customer_id);
        }
        let customer_id =
            customer_id.as_ref().ok_or_else(|| Error::ValidationError("无合同销售单必须选择客户".into()))?;
        let customer = access.load_customer(customer_id.as_ref(), &mut NoTransaction).await?;
        Ok(CustomerAccountId::new(customer.base.id))
    }

    /// 按当前有效合同修订补齐不可由客户端声明的销售草稿快照。
    ///
    /// # 参数
    /// * `access` - 已构造的命令检查器，用于合同／客户 detail 重验
    /// * `contract_id` - 合同稳定身份
    /// * `editable` - 客户端可编辑字段与行
    /// * `executor` - 调用方执行器；预装载可用 `NoTransaction`，不得代替写入事务重验
    ///
    /// # 返回
    /// 返回合同所属客户、结算主体与完整内部草稿。
    ///
    /// # 错误
    /// 合同或客户不可见、合同或修订不存在、合同非生效态、所选修订已过期时返回错误。
    ///
    /// # 关键业务约束
    /// 禁止以无范围 `find_by_id` 作为授权读取；写入事务必须再次 `related`。
    pub(super) async fn resolve_contract_sales_draft(
        &self,
        access: &SalesCommandAccess,
        contract_id: &ContractId,
        editable: SalesOrderEditableDraftRequest,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerAccountId, PartyId, SalesOrderDraftRequest)> {
        editable.validate()?;
        let revision_id = editable
            .requested_contract_revision_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("有关同时必须选择合同版本".into()))?;
        let contract = access.load_contract(contract_id.as_ref(), executor).await?;
        if contract.stable.status != ContractStatus::Effective {
            return Err(Error::BusinessLogicError("合同当前不可用于新销售提交".to_string()));
        }
        if contract.stable.current_revision_id.as_deref() != Some(revision_id.as_ref()) {
            return Err(Error::ConflictError("所选合同版本已不是当前可用版本，请刷新后重新选择".to_string()));
        }
        let revision = self
            .db
            .contract_revisions()
            .find_by_id(revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同版本不存在".to_string()))?;
        if !revision.belongs_to_contract(contract_id) {
            return Err(Error::ValidationError("合同版本不属于所选合同".to_string()));
        }
        if !revision.matches_settlement_party(&contract.settlement_party_id) {
            return Err(Error::ConflictError("合同当前结算主体与所选版本不一致，请刷新后重试".to_string()));
        }
        let customer = access.load_customer(contract.customer_id.as_ref(), executor).await?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError("客户已停用，禁止创建新销售单".to_string()));
        }
        let mut draft = SalesOrderDraftRequest {
            editor_user_id: editable.editor_user_id,
            customer_name: revision.customer_snapshot.customer_name,
            contract_no: Some(revision.contract_no),
            requested_contract_revision_id: editable.requested_contract_revision_id,
            settlement_party_name: Some(revision.settlement_party_snapshot.settlement_party_name),
            payment_term_code: revision.payment_term_snapshot.payment_term_code,
            payment_term_name: revision.payment_term_snapshot.payment_term_name,
            invoice_type: revision.invoice_requirement_snapshot.invoice_type,
            tax_point: revision.invoice_requirement_snapshot.tax_point,
            project_name: editable.project_name,
            business_remark: editable.business_remark,
            voucher_category_sku_id: editable.voucher_category_sku_id,
            voucher_expiry_at: editable.voucher_expiry_at,
            receivable_due_date: editable.receivable_due_date,
            lines: editable.lines,
        };
        draft.validate()?;
        self.sales().resolve_draft_reference_prices(&mut draft.lines, &self.catalog(), executor).await?;
        Ok((contract.customer_id, contract.settlement_party_id, draft))
    }

    /// 原子创建销售单、稳定明细及首次工作副本；`intent=SUBMIT` 时在同一写入事务
    /// 冻结首次提交、审批绑定、运行事实及成功事件。
    ///
    /// 表头金额三元组由服务端按 §4.2 铁律 2 汇总**已舍入**的行金额，客户端不可
    /// 指定；跨域校验客户（D08）与合同（D12）存在性。同一操作人使用同一幂等键
    /// 和完整载荷重试时返回原销售单；同一幂等键绑定不同载荷时返回 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回销售单详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败或行字段组缺失
    /// * `NotFound` - 客户/合同不存在
    /// * `BusinessLogicError` - 客户已停用
    /// * `ConflictError` - 单号重复或同一幂等键绑定不同载荷
    /// * `Forbidden` - 当前命令、关联对象或数据范围不允许访问
    /// * `OutcomeUnknown` - 提交结果未知且原命令结果暂无法查证
    ///
    /// # 关键业务约束
    /// 所有决定写入的关联、创建资格和可售引用在同一执行器中重验。重放只读返回
    /// 原单据；提交结果未知时只查证原回执，不自动重新执行创建。
    #[tracing::instrument(
        name = "sales_order.create",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "create")
    )]
    pub async fn create_sales_order(
        &self,
        mut req: CreateSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let access = self
            .command_access(actor, "create")?
            .require_submit(req.intent == SalesOrderCreateIntent::Submit)?;
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        req.idempotency_key.clone_from(&idempotency_key);
        let identity = CreationIdentity {
            audit_id: sales_order_create_audit_id(actor.id(), &idempotency_key),
            fingerprint: sales_order_create_fingerprint(actor.id(), &req)?,
            key_hash: CommandFingerprint::from_parts([idempotency_key.clone()]),
            idempotency_key,
        };
        if let Some(order_id) = self
            .replay_sales_order_creation(
                &identity.audit_id,
                &identity.fingerprint,
                &identity.key_hash,
                actor.id(),
                &access,
            )
            .await?
        {
            return Ok(self.read_model().sales_order_detail(&order_id, None).await?);
        }
        let creation = self.prepare_sales_creation(&req, actor, &access).await?;
        let plan = self.prepare_creation_plan(creation, req.intent, &identity, actor).await?;
        let detail_id = plan.order().base.id.clone();
        let context = self.creation_write_context(&plan, actor, &access)?;
        let transaction_result = self
            .db
            .client()
            .with_transaction(move |executor| Box::pin(persist_creation(context, plan, executor)))
            .await;
        let detail_id =
            self.creation_result(transaction_result, detail_id, &identity, actor, &access).await?;
        Ok(self.read_model().sales_order_detail(&detail_id, None).await?)
    }

    /// 只读查证失败结果，不因未知提交重新执行创建。
    async fn creation_result(
        &self,
        result: Result<Option<String>>,
        created_id: String,
        identity: &CreationIdentity,
        actor: &AuditActor,
        access: &SalesCommandAccess,
    ) -> Result<String> {
        match result {
            Ok(Some(order_id)) => Ok(order_id),
            Ok(None) => Ok(created_id),
            Err(error) => {
                self.recover_creation(
                    error,
                    (&identity.audit_id, &identity.fingerprint, &identity.key_hash),
                    actor.id(),
                    access,
                )
                .await
            },
        }
    }

    /// 查证失败建单的原命令结果；未知提交查证失败时保留首次错误来源。
    async fn recover_creation(
        &self,
        error: Error,
        identity: (&str, &str, &CommandFingerprint),
        actor_id: &str,
        access: &SalesCommandAccess,
    ) -> Result<String> {
        finish_receipt_recovery(
            error,
            self.replay_sales_order_creation(identity.0, identity.1, identity.2, actor_id, access).await,
        )
    }

    /// 用独立销售回执及当前访问资格回读原销售单，不执行建单。
    async fn replay_sales_order_creation(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_key_hash: &CommandFingerprint,
        actor_id: &str,
        access: &SalesCommandAccess,
    ) -> Result<Option<String>> {
        let db = self.db.clone();
        let command_id = audit_id.to_string();
        let fingerprint = expected_fingerprint.to_string();
        let expected_key_hash = expected_key_hash.clone();
        let actor_id = actor_id.to_string();
        let access = access.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_creation_with_executor(
                        &db,
                        &command_id,
                        (&fingerprint, &expected_key_hash),
                        &actor_id,
                        &access,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

/// 从已构造的创建事件读取原命令身份，同执行器查证原结果。
async fn replay_creation_event(
    db: &Database,
    event: &SalesCommandEvent,
    actor_id: &str,
    access: &SalesCommandAccess,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    replay_creation_with_executor(
        db,
        &event.receipt.base.id,
        (&event.receipt.fingerprint, &event.receipt.idempotency_key_hash),
        actor_id,
        access,
        executor,
    )
    .await
}

/// 用同一执行器查证完整创建身份、原销售单及创建人，然后重验当前访问。
async fn replay_creation_with_executor(
    db: &Database,
    command_id: &str,
    fingerprint: (&str, &CommandFingerprint),
    actor_id: &str,
    access: &SalesCommandAccess,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(receipt) =
        load_command_receipt(db, command_id, actor_id, "sales_order.create", None, fingerprint, executor)
            .await?
    else {
        return Ok(None);
    };
    let SalesCommandResult::Created { sales_order_id } = receipt.result else {
        return Err(Error::Internal("销售建单回执结果种类无效".to_string()));
    };
    let order = db
        .sales_orders()
        .find_by_id(sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::Internal("销售建单幂等收据对应销售单缺失".to_string()))?;
    validate_creation_fact(sales_order_id.as_ref(), &order.base.id, &order.stable.created_by, actor_id)?;
    access.current(sales_order_id.as_ref(), executor).await?;
    Ok(Some(sales_order_id.to_string()))
}

/// 原创建结果与持久化对象 ID、不可变创建人必须同时一致。
fn validate_creation_fact(
    expected_id: &str,
    persisted_id: &str,
    created_by: &str,
    actor_id: &str,
) -> Result<()> {
    if expected_id != persisted_id || created_by != actor_id {
        return Err(Error::Internal("销售建单幂等收据与创建人不一致".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生产交叉验证拒绝错销售单或被责任交接替换的创建人。
    #[test]
    fn creation_replay_preserves_original_order_and_immutable_creator() {
        validate_creation_fact("order", "order", "creator", "creator").unwrap();
        assert!(validate_creation_fact("order", "other", "creator", "creator").is_err());
        assert!(validate_creation_fact("order", "order", "current-owner", "creator").is_err());
    }
}
