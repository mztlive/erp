//! 从当前正式版本准备变更工作副本；跨域绑定和根事务由组合流程持有。

use application_core::AuditActor;
use erp_core::ids::{
    SalesChangeOrderId, SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderWorkingCopyId,
};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::SalesReviewService;
use crate::dto::sales_review::CreateSalesChangeOrderRequest;
use crate::entity::sales_order::{SalesContentHash, SalesOrderWorkingCopyLineData, WorkingPurpose};
use crate::entity::sales_review::{SalesChangeOrder, SalesChangeOrderData};
use crate::repository::prelude::*;
use crate::repository::{SalesOrderExt, SalesReviewExt};
use crate::{Error, Result};

/// 创建变更所需的销售集合写入计划；不包含审批登记或审计。
pub struct CreatedChangeWrite {
    change_order: SalesChangeOrder,
    working_copy: crate::entity::sales_order::SalesOrderWorkingCopy,
    lines: Vec<crate::entity::sales_order::SalesOrderWorkingCopyLine>,
}
impl CreatedChangeWrite {
    /// 新变更单标识，供注册审批单据与审计使用。
    pub fn change_id(&self) -> &str {
        &self.change_order.base.id
    }
    /// 绑定定义时冻结的变更单版本。
    pub fn version(&self) -> u64 {
        self.change_order.base.version
    }
    /// 从原销售单冻结的结算主体，供流程映射责任组织。
    pub fn settlement_party_id(&self) -> &erp_core::ids::PartyId {
        &self.working_copy.settlement_party_id
    }

    /// 来源销售单主键，供事务内沿原单范围重验。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回原销售单稳定身份。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 变更单不得改写该来源身份。
    pub fn sales_order_id(&self) -> &str {
        self.change_order.sales_order_id.as_ref()
    }
    /// 在绑定登记成功后，按原序写入变更单、工作副本和行，不启动事务。
    ///
    /// # 错误
    /// 保留仓储唯一键与 CAS 冲突；错误交由调用方回滚。
    pub async fn persist(&self, db: &mongodb::Database, executor: &mut dyn Executor) -> Result<()> {
        db.sales_change_orders().create(&self.change_order, executor).await?;
        db.sales_order_working_copies().create(&self.working_copy, executor).await?;
        for line in &self.lines {
            db.sales_order_working_copy_lines().create(line, executor).await?;
        }
        Ok(())
    }
}

impl SalesReviewService {
    /// 校验原销售单和基准版本，从当前正式快照准备草稿及变更工作副本。
    ///
    /// 客户端只提交变更意图；表头、合同和行事实均从冻结版本派生。
    /// 创建幂等键仅按原 DTO 校验，本方法不新增命令回执。
    ///
    /// # 错误
    /// 原销售单不可变更、基准版本漂移或已有进行中变更时保持原错误。
    pub async fn prepare_creation(
        &self,
        req: CreateSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<CreatedChangeWrite> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&req.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if let Some(blocker) = order.sales_change_start_blocker() {
            return Err(Error::BusinessLogicError(blocker.to_string()));
        }
        let base_revision_id =
            SalesOrderRevisionId::new(order.current_revision_id().expect("实体规则已确认当前版本存在"));
        let base_revision = self
            .db
            .sales_order_revisions()
            .find_by_id(base_revision_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
        if !base_revision.matches_revision_no(req.expected_base_revision_no) {
            return Err(Error::ConflictError(format!(
                "销售单当前版本已变更：期望 {}，实际 {}",
                req.expected_base_revision_no, base_revision.revision.revision_no
            )));
        }
        if self.has_in_progress_change(&req.sales_order_id, &base_revision_id).await? {
            return Err(Error::ConflictError("同一基准版本已有进行中的销售变更单".to_string()));
        }

        let change_order = SalesChangeOrder::new(
            SalesChangeOrderId::new(next_id()),
            SalesChangeOrderData {
                sales_order_id: req.sales_order_id.clone(),
                base_revision_id: base_revision_id.clone(),
                change_type: req.change_type,
                reason: req.reason,
            },
            actor.id(),
        )?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(&base_revision_id, &mut NoTransaction)
            .await?;
        let revision_line_ids = revision_lines
            .iter()
            .map(|line| SalesOrderRevisionLineId::new(line.base.id.clone()))
            .collect::<Vec<_>>();
        let goods_lines = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, &mut NoTransaction)
            .await?;
        let working_copy_id = SalesOrderWorkingCopyId::new(next_id());
        let (line_datas, lines) =
            build_change_working_copy_lines_from_revision(&working_copy_id, &revision_lines, &goods_lines)?;
        let (gross, net, tax) = crate::entity::sales_order::SalesOrderWorkingCopyLine::amount_totals(&lines);
        let working_copy = crate::entity::sales_order::SalesOrderWorkingCopy::new(
            working_copy_id,
            crate::entity::sales_order::SalesOrderWorkingCopyData {
                sales_order_id: req.sales_order_id.clone(),
                working_purpose: WorkingPurpose::SalesChange,
                sales_change_order_id: Some(change_order.base.id.clone().into()),
                base_revision_id: Some(base_revision_id.clone()),
                draft_version: 1,
                content_hash: SalesContentHash::change(&change_order.base.id, 1)?.into_wire(),
                editor_user_id: actor.id().to_string(),
                business_type: order.business_type,
                customer_id: order.customer_id.clone(),
                contract_id: order.contract_id.clone(),
                contract_revision_id: base_revision.contract_revision_id.clone(),
                settlement_party_id: order.settlement_party_id.clone(),
                snapshot: crate::entity::sales_order::HeaderSnapshotData {
                    customer_name: base_revision.customer_snapshot.customer_name.clone(),
                    contract_no: base_revision
                        .contract_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.contract_no.clone()),
                    settlement_party_name: base_revision
                        .settlement_party_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.settlement_party_name.clone()),
                    payment_term_code: base_revision.payment_term_snapshot.payment_term_code.clone(),
                    payment_term_name: base_revision.payment_term_snapshot.payment_term_name.clone(),
                    invoice_type: base_revision.invoice_requirement_snapshot.invoice_type.clone(),
                    tax_point: base_revision.invoice_requirement_snapshot.tax_point.clone(),
                },
                project_name: base_revision.project_name.clone(),
                business_remark: base_revision.business_remark.clone(),
                voucher_category_sku_id: base_revision.voucher_category_sku_id.clone(),
                voucher_expiry_at: base_revision.voucher_expiry_at,
                receivable_due_date: None,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                lines: line_datas,
            },
            actor.id(),
        )?;
        Ok(CreatedChangeWrite { change_order, working_copy, lines })
    }
    /// 同一销售单同一基准版本是否已有进行中变更。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn has_in_progress_change(
        &self,
        sales_order_id: &SalesOrderId,
        base_revision_id: &SalesOrderRevisionId,
    ) -> Result<bool> {
        Ok(self
            .db
            .sales_change_orders()
            .has_in_progress_by_order_and_base(sales_order_id, base_revision_id, &mut NoTransaction)
            .await?)
    }
}

///
/// 从当前生效销售版本构建变更工作副本行。
///
/// # 参数
/// * `working_copy_id` - 所属工作副本 ID
/// * `revision_lines` - 当前生效版本的公共行快照
/// * `goods_lines` - 当前生效版本的实物服务子行快照
///
/// # 返回
/// 返回 `(行创建数据, 行实体清单)`。
///
/// # 错误
/// 版本为空、含非实物服务行或公共行缺少子行时返回错误。
fn build_change_working_copy_lines_from_revision(
    working_copy_id: &SalesOrderWorkingCopyId,
    revision_lines: &[crate::entity::sales_order::SalesOrderRevisionLine],
    goods_lines: &[crate::entity::sales_order::SalesOrderGoodsServiceLineRevision],
) -> Result<(Vec<SalesOrderWorkingCopyLineData>, Vec<crate::entity::sales_order::SalesOrderWorkingCopyLine>)>
{
    if revision_lines.is_empty() {
        return Err(Error::ConflictError("销售单当前版本没有明细，无法发起变更".to_string()));
    }
    let mut datas = Vec::with_capacity(revision_lines.len());
    for line in revision_lines {
        let goods =
            goods_lines.iter().find(|goods| goods.revision_line_id.as_ref() == line.base.id).ok_or_else(
                || Error::ConflictError(format!("销售单当前版本第 {} 行缺少实物服务快照", line.line_no)),
            )?;
        datas.push(line.to_goods_working_copy_data(goods)?);
    }
    let mut built = Vec::with_capacity(datas.len());
    for data in &datas {
        built.push(crate::entity::sales_order::SalesOrderWorkingCopyLine::new(
            erp_core::ids::SalesOrderWorkingCopyLineId::new(next_id()),
            working_copy_id.clone(),
            data.clone(),
        )?);
    }
    Ok((datas, built))
}
