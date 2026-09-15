use erp_finance::service::payable::purchase_change::{
    PurchaseChangePayableInput, PurchaseChangePayableWrite, prepare_purchase_change_payable,
};
use erp_procurement::service::purchase_order::change::EffectiveChangeWrite;
/// 独立持有采购与财务计划的跨域写组合；采购计划内不持有财务实体。
pub(super) struct EffectiveChangePosting {
    pub(super) purchase: EffectiveChangeWrite,
    pub(super) payable: Option<PurchaseChangePayableWrite>,
}
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_procurement::dto::purchase_order::{EffectPurchaseChangeRequest, PurchaseChangeEffectResult};
use erp_procurement::entity::purchase_order::PurchaseChangeOrder;
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use mongodb::ClientSession;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::PurchaseOrderProcess;
use super::super::change_adapter::execute_purchase_change_domain_action;
use crate::{Error, Result};

impl PurchaseOrderProcess {
    /// 最终通过并生效：改写采购单并同步履约影响。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入生效。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 生效请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单/提交不存在
    /// * `ConflictError` - 版本不一致、非审批中或重复生效
    /// * `BusinessLogicError` - 基准版本已不是当前版本
    pub async fn apply_effective_change(
        &self,
        change_id: &str,
        req: EffectPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        req.validate()?;
        let change = self.domain().load_change(change_id, &mut NoTransaction).await?;
        change
            .ensure_expected_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        execute_purchase_change_domain_action(
            &mut change.clone(),
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let submission_id = change
            .submission_id_for_effect(Some(req.submission_id.as_str()))
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        self.persist_effective_change(change, submission_id.to_string(), actor).await
    }

    /// 在审批运行时持有的事务内生效采购变更。
    ///
    /// # 错误
    /// 状态、基准版本、应付/成本差额或持久化不变量失败时返回错误。
    pub async fn apply_effective_change_in_transaction(
        &self,
        change_id: &str,
        actor: &AuditActor,
        session: &mut ClientSession,
    ) -> Result<()> {
        let change = self.domain().load_change(change_id, session).await?;
        execute_purchase_change_domain_action(
            &mut change.clone(),
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let submission_id =
            change.submission_id_for_effect(None).map_err(|error| Error::ConflictError(error.to_string()))?;
        let prepared = self.prepare_effective_change_write(&change, submission_id.as_ref()).await?;
        write_effective_change_in_transaction(&self.db, prepared.write, actor, session).await.map(|_| ())
    }

    /// 准备生效修订与应付差额，并在同一事务内推进采购当前版本。
    ///
    /// # 参数
    /// * `change` - 已通过最终审批动作校验的采购变更单
    /// * `submission_id` - 当前冻结且待生效的变更提交主键
    /// * `actor` - 最终审批动作的审计操作人
    ///
    /// # 返回
    /// 返回新采购修订、应付差额引用和采购单最新乐观锁版本。
    ///
    /// # 错误
    /// 基准版本漂移、提交状态非法、销售采购 guard 并发冲突或写入失败时
    /// 返回错误。
    ///
    /// # 关键业务约束
    /// 采购修订、来源销售 guard、allocation、当前版本指针、采购任务和差额
    /// 必须原子提交。
    async fn persist_effective_change(
        &self,
        change: PurchaseChangeOrder,
        submission_id: String,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        let prepared = self.prepare_effective_change_write(&change, &submission_id).await?;
        let PreparedEffectiveChange { write, revision_id, revision_no, payable_delta_entry_id } = prepared;
        let purchase_order_lock_version = write_effective_change(&self.db, write, actor).await?;
        Ok(PurchaseChangeEffectResult {
            change_id: change.base.id.clone(),
            revision_id,
            revision_no,
            payable_delta_entry_id,
            purchase_order_lock_version,
            reference: format!("EFFECT-V{revision_no}"),
        })
    }

    /// 先完成采购冻结版本准备，再在原基准查询后的时点构造应付差额。
    ///
    /// # 错误
    /// 原采购与财务校验依次失败关闭，不提前查财务或生成其 ID。
    async fn prepare_effective_change_write(
        &self,
        change: &PurchaseChangeOrder,
        submission_id: &str,
    ) -> Result<PreparedEffectiveChange> {
        let (write, base_revision) =
            self.domain().prepare_purchase_effective_change(change, submission_id).await?;
        let payable = prepare_purchase_change_payable(PurchaseChangePayableInput {
            purchase_order_id: write.order.base.id.clone().into(),
            supplier_id: write.order.supplier_id.clone(),
            revision_id: write.revision.base.id.clone().into(),
            base_gross: base_revision.gross_amount,
            new_gross: write.revision.gross_amount,
        })?;
        Ok(PreparedEffectiveChange {
            revision_id: write.revision.base.id.clone(),
            revision_no: write.revision.revision.revision_no,
            payable_delta_entry_id: payable.as_ref().map(|value| value.entry_id().to_string()),
            write: EffectiveChangePosting { purchase: write, payable },
        })
    }
}

/// 已准备的采购变更生效事务写聚合与响应引用。
///
/// # 用途
/// 将事务写内容和事务外已确定的修订、差额引用打包，避免生效编排方法
/// 继续膨胀。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `write` 必须作为整体进入同一事务，响应引用只能来自该写聚合。
struct PreparedEffectiveChange {
    /// 完整事务写聚合。
    write: EffectiveChangePosting,
    /// 本次形成的新采购修订主键。
    revision_id: String,
    /// 本次形成的新采购修订序号。
    revision_no: u32,
    /// 本次追加的应付差额分录主键。
    payable_delta_entry_id: Option<String>,
}

/// 写入生效修订、应付差额与变更单状态。
///
/// # 用途
/// 将变更单标为已生效并提交事务写入。
///
/// # 参数
/// * `db` - 数据库
/// * `write` - 采购单、变更单、版本与差额
/// * `actor` - 审计操作人
///
/// # 返回
/// 写入成功时返回采购单更新后的乐观锁版本。
///
/// # 错误
/// 销售采购 guard、采购单或变更单 CAS 冲突时返回稳定冲突，其余仓储
/// 失败向上传递。
///
/// # 关键业务约束
/// 变更单状态迁移、来源销售 guard、allocation 和采购当前版本指针必须
/// 位于同一事务。
async fn write_effective_change(
    db: &mongodb::Database,
    write: EffectiveChangePosting,
    actor: &AuditActor,
) -> Result<u64> {
    let db = db.clone();
    let client = db.client().clone();
    let actor = actor.clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move { write_effective_change_in_transaction(&db, write, &actor, session).await })
        })
        .await
}

/// 在调用方事务内写入采购变更正式版本、差额、状态和成功审计。
///
/// # 错误
/// 状态迁移或任一仓储写入失败时返回错误。
async fn write_effective_change_in_transaction(
    db: &mongodb::Database,
    mut write: EffectiveChangePosting,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<u64> {
    let audit = actor.clone().resource_log(
        "purchase_change_order.effect",
        "purchase_change_order",
        write.purchase.change.base.id.clone(),
    )?;
    let actor_id = actor.id().to_string();
    write.purchase.mark_effective(&actor_id)?;
    super::posting::persist_effective_writes(db, write, audit, &actor_id, session).await
}
