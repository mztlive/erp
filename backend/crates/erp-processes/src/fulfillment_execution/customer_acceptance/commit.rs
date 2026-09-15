//! 幂等登记根事务：草稿/正式事实、销售进度、任务和命令收据按原顺序组合。
use application_core::{AuditActor, CommandReceipt};
use erp_audit::CommandReceiptServiceExt;
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::dto::CommitCustomerAcceptanceRequest;
use erp_fulfillment::entity::fulfillment::CustomerAcceptance;
use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment::service::document_number::next_customer_acceptance_no;
use erp_read_models::fulfillment_center::dto::CommitCustomerAcceptanceView;
use erp_sales::repository::SalesOrderExt;
use id_generator::next_id;
use persistence_core::Transactional;
use validator::Validate;

use super::CustomerAcceptanceProcess;
use super::completion::{CompletionKind, complete_acceptance};
use super::registration::register_created_customer_acceptance_document;
use super::task::prepare_customer_acceptance_task_command;
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
        if let Some(acceptance_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self
                .read
                .committed_customer_acceptance_view(&acceptance_id, &req.sales_order_id)
                .await
                .map_err(crate::Error::from);
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    let existing =
                        FulfillmentService::load_customer_acceptance_commit_draft(&db, &req, session).await?;
                    let order = db
                        .sales_orders()
                        .find_by_id(req.sales_order_id.as_ref(), session)
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
                        session,
                    )
                    .await?;
                    let (mut acceptance, is_new) = FulfillmentService::prepare_customer_acceptance_commit(
                        existing,
                        &acceptance_id,
                        &req,
                        generated_acceptance_no.clone(),
                    )?;
                    if is_new {
                        register_created_customer_acceptance_document(
                            &db,
                            &rbac,
                            object_read.as_ref(),
                            &acceptance,
                            &actor,
                            session,
                        )
                        .await?;
                    }
                    FulfillmentService::persist_customer_acceptance_commit(
                        &db,
                        &mut acceptance,
                        is_new,
                        &final_lines,
                        &req,
                        session,
                    )
                    .await?;
                    complete_acceptance(
                        &db,
                        &acceptance,
                        &actor,
                        CompletionKind::Commit { task, receipt: command_receipt_for_tx },
                        session,
                    )
                    .await?;
                    Ok::<CustomerAcceptance, crate::Error>(acceptance)
                })
            })
            .await;

        let posted = match transaction_result {
            Ok(posted) => posted,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(acceptance_id) => {
                    return self
                        .read
                        .committed_customer_acceptance_view(&acceptance_id, &sales_order_id)
                        .await
                        .map_err(crate::Error::from);
                },
                None => return Err(error),
            },
        };
        let remaining_eligibility = self.read.acceptance_eligibility(sales_order_id.as_ref()).await?;
        Ok(CommitCustomerAcceptanceView { acceptance: posted.into(), remaining_eligibility })
    }
}
