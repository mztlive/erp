//! 多份草稿、逐份成功回执及审计同事务保存；整批原载荷用于恢复。
use application_core::CommandReceipt;
use erp_finance::FinanceCommandReceiptService;
use erp_finance::dto::payable::payment_request::CreatePaymentRequestBatch;
use persistence_core::NoTransaction;

use super::*;

impl PurchasePaymentRequestProcess {
    /// 原子创建独立付款申请草稿，并按原命令恢复重复请求。
    /// # 参数
    /// * `req` - 完整批次及稳定操作号。
    /// * `actor` - 当前采购申请人。
    /// # 返回
    /// 按请求顺序返回全部已保存草稿。
    /// # 错误
    /// 任一来源、账户、金额或授权失效时整批拒绝；同号异载荷冲突。
    pub async fn create_batch(
        &self,
        req: CreatePaymentRequestBatch,
        actor: &AuditActor,
    ) -> Result<Vec<PurchasePaymentRequest>> {
        let req = req.normalize()?;
        let commands = batch_commands(&req, actor.id())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let writer = actor.clone();
        let recovery = commands.clone();
        let revision = rbac.current_policy_revision().await?;
        let result = rbac
            .clone()
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move {
                    if let Some(rows) = recover(&db, &rbac, &commands, &writer, executor).await? {
                        return Ok(rows);
                    }
                    let mut rows = Vec::with_capacity(req.requests.len());
                    for (data, command) in req.requests.into_iter().zip(&commands) {
                        let draft = PurchasePaymentRequest::new(id_generator::next_id(), data, writer.id())?;
                        let row =
                            draft::create_draft(&db, &rbac, draft, &writer, Some(command), executor).await?;
                        rows.push(row);
                    }
                    Ok(rows)
                })
            })
            .await;
        match result {
            Ok(rows) => Ok(rows),
            Err(error) => match recover(&self.db, &self.rbac, &recovery, actor, &mut NoTransaction).await? {
                Some(rows) => Ok(rows),
                None => Err(error),
            },
        }
    }
}

fn batch_commands(req: &CreatePaymentRequestBatch, actor: &str) -> Result<Vec<CommandReceipt>> {
    req.requests
        .iter()
        .enumerate()
        .map(|(index, _)| {
            // 每份命令都绑定完整批次，增删、排序或改动任一申请均属于异载荷。
            Ok(CommandReceipt::from_payload(
                "payment-request-create-",
                actor,
                "purchase_payment_request.create",
                "purchase_payment_request",
                &format!("{}:{index}", req.idempotency_key),
                &req.requests,
            )?)
        })
        .collect()
}

async fn recover(
    db: &Database,
    rbac: &SharedRbacService,
    commands: &[CommandReceipt],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<Vec<PurchasePaymentRequest>>> {
    let receipts = FinanceCommandReceiptService::new(db.clone());
    let mut rows = Vec::with_capacity(commands.len());
    for command in commands {
        let Some(id) = receipts.committed_resource_id(command, executor).await? else {
            if rows.is_empty() {
                return Ok(None);
            }
            return Err(Error::Internal("批量申请保存结果不完整".into()));
        };
        let row = load(db, &id, executor).await?;
        ensure_creator(&row, actor)?;
        require_sources(db, rbac, &row, actor, "create", executor).await?;
        rows.push(row);
    }
    Ok(Some(rows))
}

#[cfg(test)]
mod tests {
    use erp_finance::entity::command_receipt::FinanceCommandReceipt;
    use serde_json::json;

    use super::*;

    #[test]
    fn batch_receipts_bind_actor_order_and_entire_payload() {
        let mut req: CreatePaymentRequestBatch = serde_json::from_value(json!({
            "idempotency_key": "batch-1", "requests": [{
                "supplier_id": "supplier", "payee_bank_account_id": "bank",
                "lines": [{"payable_account_id": "a", "amount": "30.00"}],
                "desired_payment_date": "2026-10-15", "reason": "预付款", "attachment_ids": []
            }]
        }))
        .unwrap();
        let commands = batch_commands(&req, "buyer").unwrap();
        let receipt = FinanceCommandReceipt::resource(&commands[0], "draft".into(), "audit".into()).unwrap();
        assert_eq!(receipt.resource_id(&batch_commands(&req, "buyer").unwrap()[0]).unwrap(), "draft");
        req.requests[0].reason = "changed".into();
        assert!(receipt.resource_id(&batch_commands(&req, "buyer").unwrap()[0]).is_err());
        assert!(receipt.resource_id(&batch_commands(&req, "other").unwrap()[0]).is_err());
        req.requests.push(req.requests[0].clone());
        let changed = batch_commands(&req, "buyer").unwrap();
        assert_ne!(changed[0].id(), changed[1].id());
        assert!(receipt.resource_id(&changed[0]).is_err());
    }
}
