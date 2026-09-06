use database::{AccessControlExt, SalesOrderExt};
use entities::sales_order::{
    SalesContentHash, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyUpdate,
};
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::dto::{SaveWorkingCopyRequest, WorkingCopyView};
use super::super::mapper::{build_working_copy_lines, header_snapshot};
use super::super::SalesOrderService;
use crate::audit::AuditActorLogs;
use crate::errors::{Error, Result};
use application_core::AuditActor;

impl SalesOrderService {
    /// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁语义）。
    ///
    /// 采购/销售驳回后订单回到草稿，但首次提交工作副本已是 `Submitted` 终态时，
    /// 会新开一份 `Editing` 副本，而不是返回「有效工作副本不存在」。
    /// 已有有效副本时，`req.version` 必须与当前工作副本版本一致；新开副本不校验
    /// 该版本（前端在无副本时会把销售单版本误当成草稿版本）。
    /// 行替换在事务内「软删旧行 + 写入新行」原子完成。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 保存请求（含期望版本与草稿内容）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回保存后的工作副本视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 非草稿，或已有副本但期望版本不一致
    #[tracing::instrument(
        name = "sales_order.save_working_copy",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "save_working_copy")
    )]
    pub async fn save_working_copy(
        &self,
        id: &str,
        req: SaveWorkingCopyRequest,
        actor: &AuditActor,
    ) -> Result<WorkingCopyView> {
        req.validate()?;
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(&req.contract_id, req.draft)
            .await?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        if !order.matches_contract_context(&req.contract_id, &customer_id, &settlement_party_id) {
            return Err(Error::ConflictError(
                "销售单合同归属已变化，请刷新后重试".to_string(),
            ));
        }
        self.ensure_sellable_draft_lines(&draft.lines).await?;
        let (mut working_copy, stable, opened_new) = self
            .load_or_reopen_first_submission_working_copy(&order, req.version, &draft, actor)
            .await?;
        if opened_new {
            return self.working_copy_view(&working_copy).await;
        }

        let snapshot = header_snapshot(&draft)?;
        let created_stable_lines = stable.created;
        let lines = build_working_copy_lines(
            &order_id,
            &working_copy.base.id.clone().into(),
            &stable.all,
            &draft.lines,
        )?;
        let (gross, net, tax) = SalesOrderWorkingCopyLine::amount_totals(&lines);
        let next_version = working_copy.draft_version + 1;
        working_copy.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire()),
                customer_id: Some(order.customer_id.clone()),
                contract_id: order.contract_id.clone(),
                contract_revision_id: draft.requested_contract_revision_id.clone(),
                settlement_party_id: Some(order.settlement_party_id.clone()),
                snapshot: Some(snapshot),
                project_name: draft.project_name.clone(),
                business_remark: draft.business_remark.clone(),
                voucher_category_sku_id: draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: draft
                    .voucher_expiry_at
                    .map(|secs| Instant::from_unix_secs(secs as i64)),
                receivable_due_date: draft.receivable_due_date,
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
            },
            actor.id(),
        )?;
        working_copy.save_draft(
            SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire(),
            draft.editor_user_id.clone(),
        )?;

        let old_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&working_copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let audit = actor
            .clone()
            .resource_log("sales_order.save_draft", "sales_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let lines_for_tx = lines.clone();
        let created_stable_for_tx = created_stable_lines;
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&lines)?;
        let working_copy = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    for line in &created_stable_for_tx {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    for mut old in old_lines {
                        db.sales_order_working_copy_lines()
                            .soft_delete(&mut old, session)
                            .await?;
                    }
                    for line in &lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .update(&mut working_copy, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::errors::Error>(working_copy)
                })
            })
            .await?;

        self.working_copy_view(&working_copy).await
    }
}
