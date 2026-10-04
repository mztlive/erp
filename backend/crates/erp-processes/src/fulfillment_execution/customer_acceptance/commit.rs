//! 幂等登记根事务：草稿/正式事实、销售进度、任务和命令收据按原顺序组合。
use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::{CustomerAcceptanceId, SalesOrderId};
use erp_fulfillment::FulfillmentCommandReceiptService;
use erp_fulfillment::dto::CommitCustomerAcceptanceRequest;
use erp_fulfillment::entity::fulfillment::CustomerAcceptance;
use erp_fulfillment::repository::extensions::FulfillmentExt;
use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment::service::document_number::next_customer_acceptance_no;
use erp_identity::SharedRbacService;
use erp_read_models::fulfillment_center::dto::CommitCustomerAcceptanceView;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::ApprovalObjectReadPort;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::CustomerAcceptanceProcess;
use super::completion::{CompletionKind, complete_acceptance};
use super::evidence::ensure_evidence;
use super::registration::register_created_customer_acceptance_document;
use super::task::prepare_customer_acceptance_task_command;
use crate::reverse_flow::command_recovery::recovered_resource;
use crate::{Error, Result};
impl CustomerAcceptanceProcess {
    /// 原子登记并过账客户验收。
    ///
    /// 同一事务内完成草稿创建或完整替换、履约事实分配校验与写入、验收过账、
    /// 销售单履约进度刷新和审计。前端无需先保存草稿或预查询事实类型。
    /// 同一操作号已完成过账时直接返回既有结果，支持结果未知后的安全重试。
    ///
    /// # 参数
    /// * `req` - 最终验收表头、行、分配和乐观锁版本
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已过账验收单以及过账后重新计算的剩余可验收事实。
    ///
    /// # 错误
    /// * `ValidationError` - 行、分配、事实归属或版本参数不合法
    /// * `ConflictError` - 草稿、销售单版本或状态冲突
    /// * `NotFound` - 草稿、销售单或履约事实不存在
    /// * `OutcomeUnknown` - 事务提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_commit",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "customer_acceptance_commit")
    )]
    pub async fn commit_customer_acceptance(
        &self,
        req: CommitCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CommitCustomerAcceptanceView> {
        req.validate()?;
        erp_read_models::sales_center::access::SalesAccess::new(self.db.clone(), self.rbac.clone())
            .detail(actor, req.sales_order_id.as_ref())
            .await?;
        FulfillmentService::validate_customer_acceptance_task_context(
            req.work_item_id.as_deref(),
            req.expected_task_version,
        )?;
        if req.acceptance_id.is_some() != req.expected_acceptance_version.is_some() {
            return Err(Error::ValidationError("已有草稿必须同时提交草稿主键和期望版本".to_string()));
        }
        let command_receipt = CommandReceipt::from_payload(
            "customer-acceptance-commit-",
            actor.id(),
            "customer_acceptance.commit",
            "customer_acceptance",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(acceptance_id) = FulfillmentCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command_receipt, &mut NoTransaction)
            .await?
        {
            return self.committed_view(&acceptance_id, &req.sales_order_id).await;
        }

        let generated_acceptance_no = if req.acceptance_id.is_none() {
            Some(next_customer_acceptance_no(&self.db).await?)
        } else {
            None
        };
        let acceptance_id = req
            .acceptance_id
            .as_ref()
            .map(|id| CustomerAcceptanceId::new(id.clone()))
            .unwrap_or_else(|| CustomerAcceptanceId::new(next_id()));
        let final_lines =
            FulfillmentService::build_customer_acceptance_lines(acceptance_id.clone(), &req.lines)?;
        let sales_order_id = req.sales_order_id.clone();
        let actor = actor.clone();
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_read_models::sales_center::access::SalesAccess::new(db.clone(), rbac.clone())
                        .require_object(&actor, "detail", &req.sales_order_id, &[], executor)
                        .await?;
                    if let Some(persisted) =
                        replay_commit(&db, &command_receipt_for_tx, &req, executor).await?
                    {
                        return Ok::<CustomerAcceptance, crate::Error>(persisted);
                    }
                    let existing =
                        FulfillmentService::load_customer_acceptance_commit_draft(&db, &req, executor)
                            .await?;
                    let order = db
                        .sales_orders()
                        .find_by_id(req.sales_order_id.as_ref(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
                    if order.base.version != req.expected_sales_order_version {
                        return Err(Error::ConflictError("销售单已变化，请刷新履约事实后重试".to_string()));
                    }
                    let task = prepare_customer_acceptance_task_command(
                        &db,
                        &req.sales_order_id,
                        actor.id(),
                        req.work_item_id.as_deref(),
                        req.expected_task_version,
                        executor,
                    )
                    .await?;
                    let (mut acceptance, is_new) = FulfillmentService::prepare_customer_acceptance_commit(
                        existing,
                        &acceptance_id,
                        &req,
                        generated_acceptance_no.clone(),
                    )?;
                    prepare_document(&db, &rbac, object_read.as_ref(), &acceptance, is_new, &actor, executor)
                        .await?;
                    FulfillmentService::persist_customer_acceptance_commit(
                        &db,
                        &mut acceptance,
                        is_new,
                        &final_lines,
                        &req,
                        executor,
                    )
                    .await?;
                    complete_acceptance(
                        &db,
                        &acceptance,
                        &actor,
                        CompletionKind::Commit { task, receipt: Box::new(command_receipt_for_tx) },
                        executor,
                    )
                    .await?;
                    Ok::<CustomerAcceptance, crate::Error>(acceptance)
                })
            })
            .await;

        let posted = match transaction_result {
            Ok(posted) => posted,
            Err(error) => return self.recover_commit(error, &command_receipt, &sales_order_id).await,
        };
        let remaining_eligibility = self.read.acceptance_eligibility(sales_order_id.as_ref()).await?;
        Ok(CommitCustomerAcceptanceView { acceptance: posted.into(), remaining_eligibility })
    }

    /// 只读查询命令收据并返回既有过账视图，未确认结果时保留原错误。
    async fn recover_commit(
        &self,
        error: Error,
        receipt: &CommandReceipt,
        sales_order_id: &SalesOrderId,
    ) -> Result<CommitCustomerAcceptanceView> {
        let acceptance_id = recovered_resource(
            error,
            FulfillmentCommandReceiptService::new(self.db.clone())
                .committed_resource_id(receipt, &mut NoTransaction)
                .await
                .map_err(Error::from),
        )?;
        self.committed_view(&acceptance_id, sales_order_id).await
    }

    /// 从已过账验收事实生成同一销售单的剩余可验收视图。
    async fn committed_view(
        &self,
        acceptance_id: &str,
        sales_order_id: &SalesOrderId,
    ) -> Result<CommitCustomerAcceptanceView> {
        self.read
            .committed_customer_acceptance_view(acceptance_id, sales_order_id)
            .await
            .map_err(crate::Error::from)
    }
}

/// 在同一执行器上读取已提交命令结果，并核对其销售单归属。
async fn replay_commit(
    db: &Database,
    receipt: &CommandReceipt,
    req: &CommitCustomerAcceptanceRequest,
    executor: &mut dyn Executor,
) -> Result<Option<CustomerAcceptance>> {
    let Some(result_id) =
        FulfillmentCommandReceiptService::new(db.clone()).committed_resource_id(receipt, executor).await?
    else {
        return Ok(None);
    };
    let persisted = db
        .customer_acceptances()
        .find_by_id(&result_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("验收单不存在".to_string()))?;
    if persisted.sales_order_id != req.sales_order_id {
        return Err(Error::ConflictError("验收命令结果与销售单不一致".to_string()));
    }
    Ok(Some(persisted))
}

/// 先校验签收凭证，再按原顺序登记新验收单据。
///
/// # 参数
/// 组合根依赖、当前验收表头、新建标识、操作人和同一事务执行器。
/// # 返回
/// 凭证验证以及必要的新单据登记均成功时返回成功。
/// # 错误
/// 凭证不可用、无转授资格或新单据登记失败时返回原错误。
async fn prepare_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    acceptance: &CustomerAcceptance,
    is_new: bool,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_evidence(db, rbac, acceptance.require_evidence()?, actor, executor).await?;
    if is_new {
        register_created_customer_acceptance_document(db, rbac, object_read, acceptance, actor, executor)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use application_core::CommandReceipt;
    use erp_core::ids::FileAssetId;
    use erp_fulfillment::dto::CommitCustomerAcceptanceRequest;
    use serde_json::json;

    #[test]
    fn receipt_conflicts_when_signature_evidence_changes() {
        let mut request: CommitCustomerAcceptanceRequest = serde_json::from_value(json!({
            "sales_order_id": "sales-1", "expected_sales_order_version": 1,
            "accepted_at": 1700000000, "result": "PASSED", "lines": [],
            "idempotency_key": "signature-command", "evidence_attachment_id": "file-one"
        }))
        .unwrap();
        let first = CommandReceipt::from_payload(
            "customer-acceptance-commit-",
            "actor-1",
            "customer_acceptance.commit",
            "customer_acceptance",
            &request.idempotency_key,
            &request,
        )
        .unwrap();
        request.evidence_attachment_id = Some(FileAssetId::new("file-two"));
        let changed = CommandReceipt::from_payload(
            "customer-acceptance-commit-",
            "actor-1",
            "customer_acceptance.commit",
            "customer_acceptance",
            &request.idempotency_key,
            &request,
        )
        .unwrap();
        assert_ne!(first.fingerprint(), changed.fingerprint());
        request.evidence_attachment_id = None;
        let missing = CommandReceipt::from_payload(
            "customer-acceptance-commit-",
            "actor-1",
            "customer_acceptance.commit",
            "customer_acceptance",
            &request.idempotency_key,
            &request,
        )
        .unwrap();
        assert_ne!(first.fingerprint(), missing.fingerprint());
    }
}
