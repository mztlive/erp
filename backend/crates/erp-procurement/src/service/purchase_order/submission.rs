//! 采购草稿冻结、不可变提交行构造与本域序号读取。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::PurchaseOrderService;
use super::line_input::{build_submission_lines, compute_request_totals};
use crate::Result;
use crate::dto::purchase_order::SavePurchaseOrderLine;
use crate::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    inherit_submission_sources,
};
use crate::repository::PurchaseOrderExt;
impl PurchaseOrderService {
    /// 按草稿复制出待审核正式提交，并把 `draft_lines` 换成挂到新提交的行。
    ///
    /// 不写库，也不修改采购单指针。序号用 `NoTransaction` 读取既有提交后计算。
    ///
    /// # 参数
    /// * `order` - 当前采购单，只用于计算提交序号
    /// * `draft` - 当前草稿提交；本方法不修改它
    /// * `draft_lines` - 原地替换为新正式提交行
    /// * `actor` - 提交人
    ///
    /// # 返回
    /// 返回新的待审核正式提交。
    ///
    /// # 错误
    /// 序号读取失败时返回仓储错误；序号溢出、草稿状态或行不变式不满足时返回 `Logic`。
    pub async fn freeze_submission(
        &self,
        order: &mut PurchaseOrder,
        draft: &mut PurchaseOrderSubmission,
        draft_lines: &mut [PurchaseOrderSubmissionLine],
        actor: &AuditActor,
    ) -> Result<PurchaseOrderSubmission> {
        let next_no = self.next_submission_no(order).await?;
        let formal = PurchaseOrderSubmission::freeze_from_draft(
            PurchaseOrderSubmissionId::new(next_id()),
            next_no,
            draft,
            Instant::now(),
            actor.id(),
        )?;
        let formal_id = PurchaseOrderSubmissionId::new(formal.base.id.clone());
        for line in draft_lines.iter_mut() {
            *line = PurchaseOrderSubmissionLine::freeze_from_draft(
                PurchaseOrderSubmissionLineId::new(next_id()),
                formal_id.clone(),
                line,
            )?;
        }
        let _ = order;
        Ok(formal)
    }
    /// 按提交命令携带的草稿补丁构造正式提交，并继承原行正式选源。
    ///
    /// # 参数
    /// * `order` - 当前采购单。
    /// * `draft` - 当前服务端草稿提交。
    /// * `draft_lines` - 同一草稿的完整原行，选源事实不得由客户端提供。
    /// * `requested_lines` - 已限定原来源的采购内容补丁。
    /// * `actor` - 当前内部提交人。
    /// # 返回
    /// 返回冻结提交与金额、数量已校验且正式选源保持不变的行。
    /// # 错误
    /// 原行缺失、SKU来源错配、序号、内容或金额校验失败时拒绝。
    pub async fn freeze_submission_from_lines(
        &self,
        order: &PurchaseOrder,
        draft: &PurchaseOrderSubmission,
        draft_lines: &[PurchaseOrderSubmissionLine],
        requested_lines: &[SavePurchaseOrderLine],
        actor: &AuditActor,
    ) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
        let next_no = self.next_submission_no(order).await?;
        let mut inputs = SavePurchaseOrderLine::to_line_inputs(requested_lines)?;
        inherit_submission_sources(&mut inputs, draft_lines)?;
        let (gross, net, tax) = compute_request_totals(&inputs)?;
        let mut formal = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: next_no,
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )?;
        formal.submit(Instant::now(), actor.id())?;
        let lines = build_submission_lines(&formal.base.id.clone().into(), &inputs)?;
        Ok((formal, lines))
    }
    /// 计算下一个提交序号（`SUB-{n}`，聚合内唯一）。
    ///
    /// 用 `NoTransaction` 读取该采购单已有提交，不加入调用方事务。
    ///
    /// # 参数
    /// * `order` - 当前采购单
    ///
    /// # 返回
    /// 返回下一个 `SUB-` 加六位序号。
    ///
    /// # 错误
    /// 提交列表读取失败时返回仓储错误；序号达到 `u32::MAX` 时返回 `Logic`。
    pub async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order()
            .list_submissions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderSubmission::next_submission_no(&existing).map_err(Into::into)
    }
}
/// 首次提交分配不可复用正式号。已有正式号时保持不变。
///
/// # 参数
/// * `order` - 待分配单号的采购单；空号时写成 `PO-` 加单据 ID
///
/// # 返回
/// 已有正式号或新号写入成功时无返回值。
///
/// # 错误
/// 生成的单号为空或超长时，实体错误经 `?` 变为 `Logic`。已有正式号不会进入该校验。
pub fn assign_formal_purchase_no(order: &mut PurchaseOrder) -> Result<()> {
    if !order.purchase_no.is_empty() {
        return Ok(());
    }
    Ok(order.assign_purchase_no(format!("PO-{}", order.base.id))?)
}
