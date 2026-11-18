//! 幂等登记根事务：草稿/正式事实、销售进度、任务和命令收据按原顺序组合。
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::Error as CoreError;
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::dto::CommitCustomerAcceptanceRequest;
use erp_fulfillment::entity::fulfillment::{CustomerAcceptance, CustomerAcceptanceLine};
use erp_fulfillment::service::FulfillmentService;
use erp_identity::AuthorizationService;
use erp_permissions::sales_order as sales_order_permissions;
use erp_read_models::fulfillment_center::FulfillmentReadService;
use erp_read_models::fulfillment_center::dto::CommitCustomerAcceptanceView;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::repository::SalesOrderExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CustomerAcceptanceProcess;
use super::completion::{CompletionKind, complete_acceptance};
use super::evidence::ensure_evidence;
use super::number::CreationNumber;
use super::registration::register_created_customer_acceptance_document;
use super::replay::{ReplayReader, committed, recover};
use super::task::prepare_customer_acceptance_task_command;
use crate::workflow::FulfillmentWorkflow;
use crate::{Error, Result};
impl CustomerAcceptanceProcess {
    /// 客户最终确认验收时直接登记并过账，无需先保存草稿或查询事实类型。原事务按最终表头、行、分配及乐观锁版本
    /// 新建或替换草稿，校验并写入履约事实分配，再过账、刷新销售履约进度并审计。
    ///
    /// 按原操作号查证和重放，不再次过账。返回已过账验收单及重算后的剩余可验收事实。
    /// 行、分配、事实归属或版本参数不合法返回 `ValidationError`；草稿、销售版本或状态冲突返回 `ConflictError`；
    /// 草稿、销售单或履约事实缺失返回 `NotFound`；无法确认事务结果返回 `OutcomeUnknown`。
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
        let command_receipt = self.prepare_commit_request(&req, actor)?;
        if let Some(view) = self.committed_result(&req, actor, &command_receipt).await? {
            return Ok(view);
        }

        let command = PreparedAcceptanceCommit::new(self, req.clone(), actor, command_receipt.clone())?;
        let transaction_result = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| Box::pin(command.execute(executor)))
            .await;

        match transaction_result {
            Ok(view) => Ok(view),
            Err(error) => self.recover_commit(error, &command_receipt, &req, actor).await,
        }
    }

    fn prepare_commit_request(
        &self,
        req: &CommitCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CommandReceipt> {
        req.validate()?;
        FulfillmentService::validate_customer_acceptance_task_context(
            req.work_item_id.as_deref(),
            req.expected_task_version,
        )?;
        if req.acceptance_id.is_some() != req.expected_acceptance_version.is_some() {
            return Err(Error::ValidationError("已有草稿必须同时提交草稿主键和期望版本".to_string()));
        }
        commit_command_receipt(req, actor.id())
    }

    // 写事务失败后只按原请求和回执查证；未确认成功时保留原错误，不再次执行验收过账。
    async fn recover_commit(
        &self,
        error: Error,
        receipt: &CommandReceipt,
        req: &CommitCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CommitCustomerAcceptanceView> {
        recover(error, self.committed_result(req, actor, receipt)).await
    }

    // 重试或提交查证时，在同一恢复快照核对原回执、真实验收任务和销售来源，再按原验收单读取完整结果；无原回执时不交付视图。
    async fn committed_result(
        &self,
        req: &CommitCustomerAcceptanceRequest,
        actor: &AuditActor,
        receipt: &CommandReceipt,
    ) -> Result<Option<CommitCustomerAcceptanceView>> {
        let reader = ReplayReader { db: self.db.clone(), authorization: self.authorization.clone() };
        let (req, actor, receipt) = (req.clone(), actor.clone(), receipt.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Some(acceptance) = committed(&reader, &req, &actor, &receipt, executor).await? else {
                        return Ok(None);
                    };
                    let view = FulfillmentReadService::new(reader.db.clone())
                        .committed_customer_acceptance_view(
                            &acceptance.base.id,
                            &req.sales_order_id,
                            executor,
                        )
                        .await?;
                    Ok::<_, Error>(Some(view))
                })
            })
            .await
    }
}

pub(super) fn commit_command_receipt(
    req: &CommitCustomerAcceptanceRequest,
    actor_id: &str,
) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_payload(
        "customer-acceptance-commit-",
        actor_id,
        "customer_acceptance.commit",
        "customer_acceptance",
        &req.idempotency_key,
        req,
    )?)
}

/// 根事务开始前固定行身份及命令依赖，恢复查询仍使用原命令收据。
struct PreparedAcceptanceCommit {
    db: Database,
    authorization: Arc<AuthorizationService>,
    workflow: Arc<dyn FulfillmentWorkflow>,
    req: CommitCustomerAcceptanceRequest,
    actor: AuditActor,
    receipt: CommandReceipt,
    acceptance_id: CustomerAcceptanceId,
    final_lines: Vec<CustomerAcceptanceLine>,
}

impl PreparedAcceptanceCommit {
    fn new(
        process: &CustomerAcceptanceProcess,
        req: CommitCustomerAcceptanceRequest,
        actor: &AuditActor,
        receipt: CommandReceipt,
    ) -> Result<Self> {
        let acceptance_id = req
            .acceptance_id
            .as_ref()
            .map(|id| CustomerAcceptanceId::new(id.clone()))
            .unwrap_or_else(|| CustomerAcceptanceId::new(next_id()));
        let final_lines =
            FulfillmentService::build_customer_acceptance_lines(acceptance_id.clone(), &req.lines)?;
        Ok(Self {
            db: process.db.clone(),
            authorization: process.authorization.clone(),
            workflow: process.workflow.clone(),
            req,
            actor: actor.clone(),
            receipt,
            acceptance_id,
            final_lines,
        })
    }

    async fn execute(self, executor: &mut dyn Executor) -> Result<CommitCustomerAcceptanceView> {
        let reader = ReplayReader { db: self.db.clone(), authorization: self.authorization.clone() };
        if let Some(persisted) = committed(&reader, &self.req, &self.actor, &self.receipt, executor).await? {
            return Ok(FulfillmentReadService::new(self.db.clone())
                .committed_customer_acceptance_view(&persisted.base.id, &self.req.sales_order_id, executor)
                .await?);
        }

        SalesAccess::new(self.db.clone(), self.authorization.clone())
            .require_object(&self.actor, &sales_order_permissions::DETAIL, &self.req.sales_order_id, executor)
            .await?;

        let existing =
            FulfillmentService::load_customer_acceptance_commit_draft(&self.db, &self.req, executor).await?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(self.req.sales_order_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != self.req.expected_sales_order_version {
            return Err(Error::ConflictError("销售单已变化，请刷新后重新核对履约记录".to_string()));
        }

        self.persist(existing, executor).await
    }

    async fn persist(
        self,
        existing: Option<CustomerAcceptance>,
        executor: &mut dyn Executor,
    ) -> Result<CommitCustomerAcceptanceView> {
        let task = prepare_customer_acceptance_task_command(
            &self.db,
            &self.authorization,
            &self.req.sales_order_id,
            self.actor.id(),
            self.req.work_item_id.as_deref(),
            self.req.expected_task_version,
            executor,
        )
        .await?;

        let generated_acceptance_no = self.allocate_number(&existing, executor).await?;
        let (mut acceptance, is_new) = FulfillmentService::prepare_customer_acceptance_commit(
            existing,
            &self.acceptance_id,
            &self.req,
            generated_acceptance_no,
        )?;
        self.write(&mut acceptance, is_new, executor).await?;

        complete_acceptance(
            &self.db,
            &self.authorization,
            &acceptance,
            &self.actor,
            CompletionKind::Commit { task, receipt: Box::new(self.receipt) },
            executor,
        )
        .await?;

        Ok(FulfillmentReadService::new(self.db.clone())
            .committed_customer_acceptance_view(&acceptance.base.id, &self.req.sales_order_id, executor)
            .await?)
    }

    // 新建时必须已通过任务资格，再完成创建及凭证授权后消费独立编号。
    async fn allocate_number(
        &self,
        existing: &Option<CustomerAcceptance>,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        if existing.is_some() {
            return Ok(None);
        }
        let evidence = self
            .req
            .evidence_attachment_id
            .as_ref()
            .filter(|id| !id.as_ref().trim().is_empty())
            .ok_or_else(|| CoreError::from("请上传签收单凭证（图片或 PDF）"))?;
        let number = CreationNumber {
            db: &self.db,
            authorization: &self.authorization,
            workflow: self.workflow.as_ref(),
            sales_order_id: &self.req.sales_order_id,
            actor: &self.actor,
            evidence: Some(evidence),
        }
        .allocate(executor)
        .await?;
        Ok(Some(number))
    }

    // 单据注册与验收事实写入沿原命令执行器完成。
    async fn write(
        &self,
        acceptance: &mut CustomerAcceptance,
        is_new: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let document =
            CustomerAcceptanceDocument { db: &self.db, workflow: self.workflow.as_ref(), actor: &self.actor };
        document.prepare(acceptance, is_new, executor).await?;

        FulfillmentService::persist_customer_acceptance_commit(
            &self.db,
            acceptance,
            is_new,
            &self.final_lines,
            &self.req,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 当前验收命令登记单据时使用的原授权、范围和操作身份。
struct CustomerAcceptanceDocument<'a> {
    db: &'a Database,
    workflow: &'a dyn FulfillmentWorkflow,
    actor: &'a AuditActor,
}

impl CustomerAcceptanceDocument<'_> {
    // 新单沿取号前已校验的资格登记；既有草稿在写入前校验最终凭证。
    async fn prepare(
        &self,
        acceptance: &CustomerAcceptance,
        is_new: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if is_new {
            register_created_customer_acceptance_document(
                self.db,
                self.workflow,
                acceptance,
                self.actor,
                executor,
            )
            .await?;
        } else {
            ensure_evidence(self.db, self.workflow, acceptance.require_evidence()?, self.actor, executor)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::FileAssetId;
    use erp_fulfillment::dto::CommitCustomerAcceptanceRequest;
    use serde_json::json;

    use super::commit_command_receipt;

    #[test]
    fn receipt_conflicts_when_signature_evidence_changes() {
        let mut request: CommitCustomerAcceptanceRequest = serde_json::from_value(json!({
            "sales_order_id": "sales-1", "expected_sales_order_version": 1,
            "accepted_at": 1700000000, "result": "PASSED", "lines": [],
            "idempotency_key": "signature-command", "evidence_attachment_id": "file-one"
        }))
        .unwrap();
        let first = commit_command_receipt(&request, "actor-1").unwrap();
        let replay = commit_command_receipt(&request, "actor-1").unwrap();
        assert_eq!(first.fingerprint(), replay.fingerprint());

        request.evidence_attachment_id = Some(FileAssetId::new("file-two"));
        let changed = commit_command_receipt(&request, "actor-1").unwrap();
        assert_ne!(first.fingerprint(), changed.fingerprint());

        request.evidence_attachment_id = None;
        let missing = commit_command_receipt(&request, "actor-1").unwrap();
        assert_ne!(first.fingerprint(), missing.fingerprint());
    }
}
