//! 人工保存采购草稿。

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt, Transactional};
use entities::ids::PurchaseOrderSubmissionId;
use entities::purchase_order::{
    PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData, SubmissionStatus,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{SavePurchaseOrderDraftRequest, SavePurchaseOrderDraftResult, TotalsView};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 保存采购草稿（表头 + 完整行替换，单事务）。
    ///
    /// 只允许 `Draft` 状态；期望版本与当前版本不一致返回 409。
    /// 每次保存形成新的草稿提交（行集合不可变替换，旧草稿提交标记失效），
    /// 商品行金额按 `line_amounts(单价, 数量, 税率)` 逐行计算，
    /// 物流费用行按 `gross − round(gross × 税率)` 换算，表头只汇总已舍入行金额。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 保存请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新乐观锁版本与表头汇总。
    ///
    /// # 错误
    /// * `NotFound` - 采购单或草稿提交不存在
    /// * `ConflictError` - 期望版本不一致
    /// * `BusinessLogicError` - 状态非草稿或行数据非法
    pub async fn save_draft(
        &self,
        id: &str,
        req: SavePurchaseOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<SavePurchaseOrderDraftResult> {
        req.validate()?;
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Draft {
            return Err(Error::BusinessLogicError(
                "只有草稿状态的采购单可以编辑".to_string(),
            ));
        }
        let old_draft_id = order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
        let mut old_draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&old_draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        if old_draft.status != SubmissionStatus::Draft {
            return Err(Error::BusinessLogicError("草稿提交已冻结，不能保存".to_string()));
        }

        let (gross, net, tax) = self.compute_request_totals(&req.lines).await?;
        let mut update = entities::purchase_order::PurchaseOrderUpdate::default();
        if let Some(payment_term_code) = req.payment_term_code.clone() {
            update.payment_term_code = Some(payment_term_code);
        }
        order.update(update, actor.id())?;
        let new_draft = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: format!("DRAFT-{}", &next_id()[..8]),
                supplier_id: old_draft.supplier_id.clone(),
                purchase_type: old_draft.purchase_type,
                fulfillment_responsibility: old_draft.fulfillment_responsibility,
                supplier_revision_id: old_draft.supplier_revision_id.clone(),
                supplier_snapshot: old_draft.supplier_snapshot.clone(),
                payment_term_snapshot: old_draft.payment_term_snapshot.clone(),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )?;
        let lines = self
            .build_lines_from_request(&new_draft.base.id.clone().into(), &req.lines)
            .await?;
        old_draft.mark_superseded()?;
        order.current_submission_id = Some(new_draft.base.id.clone());

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.update", "purchase_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let old_draft_for_tx = old_draft.clone();
        let new_draft_for_tx = new_draft.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order_submissions()
                        .update(&mut old_draft_for_tx.clone(), session)
                        .await?;
                    db.purchase_order_submissions()
                        .create(&new_draft_for_tx, session)
                        .await?;
                    for line in &lines {
                        db.purchase_order_submission_lines().create(line, session).await?;
                    }
                    db.purchase_orders()
                        .update(&mut order_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(SavePurchaseOrderDraftResult {
            lock_version: order.base.version,
            totals: TotalsView {
                gross: gross.to_string(),
                net: net.to_string(),
                tax: tax.to_string(),
            },
            reference: format!("SAVED-V{}", order.base.version),
        })
    }
}
