//! 采购草稿冻结、提交审核与待办构造。

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::{PurchaseOrderSubmissionId, WorkItemId};
use entities::purchase_order::{
    PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
    PurchaseOrderSubmissionLine, SubmissionStatus,
};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use validator::Validate;

use super::dto::{SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 提交财务审核（§6.6：头行冻结，形成不可变提交与审核待办）。
    ///
    /// 单事务写入提交、提交明细、采购主表指针与审核待办；重复提交（状态已
    /// 非草稿）直接失败，提交序号唯一索引兜底，只产生一条正式提交。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交结果（提交 ID、序号与审核待办）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 期望版本不一致或重复提交
    /// * `BusinessLogicError` - 状态非草稿或草稿内容缺失
    pub async fn submit(
        &self,
        id: &str,
        req: SubmitPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmitPurchaseOrderResult> {
        req.validate()?;
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Draft {
            return Err(Error::ConflictError(
                "采购单已提交或已生效，请勿重复提交".to_string(),
            ));
        }
        let draft_id = order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
        let mut draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        if draft.status != SubmissionStatus::Draft {
            return Err(Error::ConflictError("草稿提交已冻结".to_string()));
        }
        let mut draft_lines = self
            .db
            .purchase_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_submission_id": draft_id },
                &mut NoTransaction,
            )
            .await?;

        // 形成新的正式提交（复制草稿内容，冻结）。
        let mut superseded_draft = draft.clone();
        superseded_draft.mark_superseded()?;
        let submission = self
            .freeze_submission(&mut order, &mut draft, &mut draft_lines, actor)
            .await?;
        let work_item = self.build_review_work_item(&order, &submission)?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.submit", "purchase_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let work_item_for_tx = work_item.clone();
        let submission_for_tx = submission.clone();
        let superseded_draft_for_tx = superseded_draft.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_purchase_submission(
                            &mut order_for_tx.clone(),
                            &submission_for_tx,
                            &draft_lines,
                            session,
                        )
                        .await?;
                    db.purchase_order_submissions()
                        .update(&mut superseded_draft_for_tx.clone(), session)
                        .await?;
                    db.work_items().create(&work_item_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(SubmitPurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            submission_id: submission.base.id.clone(),
            submission_no: submission.submission_no.clone(),
            work_item_id: work_item.base.id.clone(),
            lock_version: order.base.version,
            reference: format!("SUB-{}", submission.submission_no),
        })
    }

    /// 冻结草稿为正式提交（复制明细并重指向正式提交、推进主表指针）。
    async fn freeze_submission(
        &self,
        order: &mut PurchaseOrder,
        draft: &mut PurchaseOrderSubmission,
        draft_lines: &mut [PurchaseOrderSubmissionLine],
        actor: &AuditActor,
    ) -> Result<PurchaseOrderSubmission> {
        let next_no = self.next_submission_no(order).await?;
        let formal = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: next_no.clone(),
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: draft.gross_amount,
                net_amount: draft.net_amount,
                tax_amount: draft.tax_amount,
            },
        )?;
        let mut formal = formal;
        formal.submit(Instant::now(), actor.id())?;
        // 正式提交行沿用草稿明细（行内提交引用改为正式提交）。
        for line in draft_lines.iter_mut() {
            line.purchase_order_submission_id = formal.base.id.clone().into();
        }
        order.submit_for_review(formal.base.id.clone(), actor.id())?;
        Ok(formal)
    }

    /// 计算下一个提交序号（`SUB-{n}`，聚合内唯一）。
    async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order_submissions()
            .find_many(
                mongodb::bson::doc! { "purchase_order_id": order.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let max_no = existing
            .iter()
            .filter_map(|submission| {
                submission
                    .submission_no
                    .strip_prefix("SUB-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("SUB-{:06}", max_no + 1))
    }

    /// 构建审核待办（D03）。
    fn build_review_work_item(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
    ) -> Result<WorkItem> {
        WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseOrderReview,
                business_object_type: "purchase_order".to_string(),
                business_object_id: order.base.id.clone(),
                subject_version: Some(submission.submission_no.clone()),
                owner_role: Some("finance".to_string()),
                owner_user_id: None,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: Some(format!("采购单 {} 待财务审核", order.purchase_no)),
                completion_action: "review".to_string(),
            },
        )
        .map_err(Into::into)
    }
}
