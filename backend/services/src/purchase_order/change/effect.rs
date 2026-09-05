use database::{
    AccessControlExt, CostExt, NoTransaction, PayableExt, PurchaseOrderExt, SalesOrderExt, Transactional,
};
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseOrder, PurchaseOrderRevision,
};
use mongodb::ClientSession;
use validator::Validate;

use super::super::allocation_maintenance::{
    persist_current_sales_allocations, prepare_current_sales_allocations,
};
use super::super::change_adapter::execute_purchase_change_domain_action;
use super::super::dto::{EffectPurchaseChangeRequest, PurchaseChangeEffectResult};
use super::super::procurement_task_sync::sync_procurement_tasks_for_sales_order;
use super::super::PurchaseOrderService;
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
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
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
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
        self.persist_effective_change(change, submission_id.to_string(), actor)
            .await
    }

    /// 在审批运行时持有的事务内生效采购变更。
    ///
    /// # 错误
    /// 状态、基准版本、应付/成本差额或持久化不变量失败时返回错误。
    pub(crate) async fn apply_effective_change_in_transaction(
        &self,
        change_id: &str,
        actor: &AuditActor,
        session: &mut ClientSession,
    ) -> Result<()> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        execute_purchase_change_domain_action(
            &mut change.clone(),
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            actor.id(),
        )?;
        let submission_id = change
            .submission_id_for_effect(None)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        let prepared = self
            .prepare_effective_change_write(&change, submission_id.as_ref())
            .await?;
        write_effective_change_in_transaction(&self.db, prepared.write, actor, session)
            .await
            .map(|_| ())
    }

    /// 客户端直接生效失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_effect() -> Result<PurchaseChangeEffectResult> {
        Err(Error::ConflictError(
            "采购变更生效只能由审批最终通过动作执行，客户端不得直接生效".to_string(),
        ))
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
        let prepared = self
            .prepare_effective_change_write(&change, &submission_id)
            .await?;
        let PreparedEffectiveChange {
            write,
            revision_id,
            revision_no,
            payable_delta_entry_id,
        } = prepared;
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

    /// 准备采购变更生效事务所需的修订、差额和响应引用。
    ///
    /// # 参数
    /// * `change` - 已通过最终审批动作校验的采购变更单
    /// * `submission_id` - 当前冻结且待生效的变更提交主键
    ///
    /// # 返回
    /// 返回可直接进入事务的完整写聚合及响应所需稳定引用。
    ///
    /// # 错误
    /// 原采购单、基准版本或提交缺失，基准版本漂移，或修订与差额构建
    /// 失败时返回错误。
    ///
    /// # 关键业务约束
    /// 这里只准备不可变写内容；采购、销售 guard、allocation 和任务的可见性
    /// 由后续事务保证。
    async fn prepare_effective_change_write(
        &self,
        change: &PurchaseChangeOrder,
        submission_id: &str,
    ) -> Result<PreparedEffectiveChange> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        change
            .ensure_base_revision_current(order.stable.current_revision_id.as_deref())
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let (submission, lines) = self.load_pending_change_submission(submission_id).await?;
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_change_revision(&order, &submission, &lines, revision_no)
            .await?;
        let delta = self
            .build_effective_change_delta(change, &order, &revision)
            .await?;
        Ok(PreparedEffectiveChange {
            revision_id: revision.base.id.clone(),
            revision_no,
            payable_delta_entry_id: delta.0.as_ref().map(|(_, entry)| entry.base.id.clone()),
            write: EffectiveChangeWrite {
                order,
                change: change.clone(),
                submission,
                revision,
                revision_lines,
                delta,
            },
        })
    }

    /// 基于采购变更基准版本和目标版本构建应付与成本差额。
    ///
    /// # 参数
    /// * `change` - 提供基准采购修订引用的采购变更单
    /// * `order` - 当前原采购单
    /// * `revision` - 本次待生效的新采购修订
    ///
    /// # 返回
    /// 返回可在生效事务中追加的应付差额与成本差额。
    ///
    /// # 错误
    /// 基准版本不存在，或差额构建所需仓储事实缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 差额只基于变更冻结的基准版本和本次目标版本计算，不采用可变草稿事实。
    async fn build_effective_change_delta(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        revision: &PurchaseOrderRevision,
    ) -> Result<EffectiveChangeDelta> {
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        self.build_change_deltas(order, &base_revision, revision).await
    }

    /// 加载待生效的变更提交及其明细。
    ///
    /// # 错误
    /// 提交不存在或已处理时返回错误。
    async fn load_pending_change_submission(
        &self,
        submission_id: &str,
    ) -> Result<(
        PurchaseChangeSubmission,
        Vec<entities::purchase_order::PurchaseChangeSubmissionLine>,
    )> {
        let submission = self
            .db
            .purchase_change_submissions()
            .find_by_id(submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
        submission
            .ensure_pending()
            .map_err(|_| Error::ConflictError("变更提交已处理，请勿重复生效".to_string()))?;
        let lines = self
            .db
            .purchase_order()
            .list_change_submission_lines(&submission.base.id.clone().into(), &mut NoTransaction)
            .await?;
        Ok((submission, lines))
    }
}

/// 采购变更生效时一次性追加的应付与成本差额。
type EffectiveChangeDelta = (
    Option<(entities::payable::PayableAccount, entities::payable::PayableEntry)>,
    Vec<entities::cost::CostEntry>,
);

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
    write: EffectiveChangeWrite,
    /// 本次形成的新采购修订主键。
    revision_id: String,
    /// 本次形成的新采购修订序号。
    revision_no: u32,
    /// 本次追加的应付差额分录主键。
    payable_delta_entry_id: Option<String>,
}

/// 采购变更生效写入所需的单据、版本与差额。
///
/// # 用途
/// 将采购单、变更单、提交、生效版本与应付/成本差额打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 生效版本必须基于当前基准版本；差额可为空。
struct EffectiveChangeWrite {
    /// 原采购单。
    order: PurchaseOrder,
    /// 待生效的变更单。
    change: PurchaseChangeOrder,
    /// 待通过的变更提交。
    submission: PurchaseChangeSubmission,
    /// 生效版本。
    revision: entities::purchase_order::PurchaseOrderRevision,
    /// 生效版本行。
    revision_lines: Vec<entities::purchase_order::PurchaseOrderRevisionLine>,
    /// 应付差额与成本差额。
    delta: EffectiveChangeDelta,
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
    write: EffectiveChangeWrite,
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
    mut write: EffectiveChangeWrite,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<u64> {
    let audit = actor.clone().resource_log(
        "purchase_change_order.effect",
        "purchase_change_order",
        write.change.base.id.clone(),
    )?;
    let actor_id = actor.id().to_string();
    write
        .change
        .apply_effective(write.revision.base.id.clone().into(), &actor_id)?;
    persist_effective_writes(db, write, audit, &actor_id, session).await
}

/// 事务内写入生效修订、指针、差额与变更单。
///
/// # 用途
/// 在已开启事务中落库生效版本、差额与变更结论。
///
/// # 参数
/// * `db` - 数据库
/// * `write` - 采购单、变更单、版本与差额
/// * `audit` - 已构造审计
/// * `actor_id` - 推进来源销售采购 guard 的最终动作账号
/// * `session` - 事务会话
///
/// # 返回
/// 写入成功时返回采购单更新后的乐观锁版本。
///
/// # 错误
/// 来源销售单缺失、任一 CAS 并发冲突或其他仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 先推进来源销售 guard，再按当前销售版本重建 allocation，最后切换采购
/// 当前版本并同步任务。
async fn persist_effective_writes(
    db: &mongodb::Database,
    write: EffectiveChangeWrite,
    audit: entities::AuditLog,
    actor_id: &str,
    session: &mut ClientSession,
) -> Result<u64> {
    let EffectiveChangeWrite {
        mut order,
        mut change,
        mut submission,
        revision,
        mut revision_lines,
        delta,
    } = write;
    advance_source_sales_procurement_guard(db, &order, actor_id, session).await?;
    let allocations = prepare_current_sales_allocations(db, &order, &mut revision_lines, session).await?;
    db.purchase_order()
        .create_effective_revision(&revision, &revision_lines, session)
        .await?;
    persist_current_sales_allocations(db, &allocations, session).await?;
    order.apply_change_revision(revision.base.id.clone().into(), actor_id)?;
    db.purchase_orders().update(&mut order, session).await?;
    sync_procurement_tasks_for_sales_order(db, &order.sales_order_id, session).await?;
    persist_effective_change_delta(db, &delta, session).await?;
    submission.approve()?;
    db.purchase_change_submissions()
        .update(&mut submission, session)
        .await?;
    db.purchase_change_orders().update(&mut change, session).await?;
    db.audit_logs().create(&audit, session).await?;
    Ok(order.base.version)
}

/// 在采购变更生效事务内追加应付与成本差额。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `delta` - 基于冻结基准版本和目标版本计算的差额
/// * `session` - 与采购修订和当前版本指针共用的事务会话
///
/// # 返回
/// 全部差额写入成功时返回 `Ok(())`。
///
/// # 错误
/// 应付或成本事实写入失败时返回错误。
///
/// # 关键业务约束
/// 差额不得先于采购修订独立提交，任一失败必须回滚整个采购变更生效事务。
async fn persist_effective_change_delta(
    db: &mongodb::Database,
    delta: &EffectiveChangeDelta,
    session: &mut ClientSession,
) -> Result<()> {
    if let Some((account, entry)) = &delta.0 {
        db.payable()
            .create_payable_with_entry(account, entry, session)
            .await?;
    }
    for entry in &delta.1 {
        db.cost()
            .create_cost_entry_with_allocations(entry, Vec::new(), session)
            .await?;
    }
    Ok(())
}

/// 在采购变更生效事务内推进来源销售单的采购串行化 guard。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 当前待切换生效版本的采购单
/// * `actor_id` - 最终审批动作账号
/// * `session` - 与采购修订、allocation 和任务同步共用的事务会话
///
/// # 返回
/// 来源销售单 CAS 更新成功时返回 `Ok(())`。
///
/// # 错误
/// 来源销售单不存在、guard 溢出、乐观锁或瞬态事务冲突时返回错误。
///
/// # 关键业务约束
/// 必须先通过销售单 `id + version` CAS 推进 `procurement_guard_version`，
/// 后续才能重算采购覆盖。
async fn advance_source_sales_procurement_guard(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    actor_id: &str,
    session: &mut ClientSession,
) -> Result<()> {
    let mut sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    sales_order.advance_procurement_guard(actor_id)?;
    db.sales_orders().update(&mut sales_order, session).await?;
    Ok(())
}
