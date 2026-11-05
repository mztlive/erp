//! 补充或调整地址的根事务：独立安排、未发快照、任务版本与审计原子更新。
use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::validation::normalize_required_text;
use erp_fulfillment::entity::address_arrangement::{
    AddressArrangement, AddressPool, ArrangementLine, requires_change_reason, validate_arrangement,
};
use erp_fulfillment::entity::fulfillment::DeliveryAddressSnapshot;
use erp_fulfillment::repository::{AddressArrangementExt, FulfillmentExt};
use erp_sales::repository::SalesOrderExt;
use erp_workflow::WorkItemExt;
use persistence_core::{Executor, Transactional};

use super::super::{FulfillmentProcess, delivery_addresses, task};
use super::SaveAddressArrangement;
use super::context::{self, Context};
use crate::{Error, Result};

impl FulfillmentProcess {
    /// 保存销售单收货安排；只影响未发部分，不创建商业改单。
    /// # 参数
    /// 销售单身份、完整地址输入及认证操作人。
    /// # 返回
    /// 成功后由客户端重新读取安排和履约任务。
    /// # 错误
    /// 范围、版本、数量、已发保护或包裹重核确认不满足时整笔回滚。
    pub async fn save_sales_address_arrangement(
        &self,
        id: &str,
        req: SaveAddressArrangement,
        actor: &AuditActor,
    ) -> Result<()> {
        self.save_address_arrangement(id, false, req, actor).await
    }
    /// 当前任务处理人补充本任务地址，不获得其他任务的编辑权限。
    /// # 参数
    /// 发货身份、完整任务地址输入及认证操作人。
    /// # 返回
    /// 保存成功。
    /// # 错误
    /// 非当前处理人、已发货、版本冲突或地址数量非法时回滚。
    pub async fn save_delivery_address_arrangement(
        &self,
        id: &str,
        req: SaveAddressArrangement,
        actor: &AuditActor,
    ) -> Result<()> {
        self.save_address_arrangement(id, true, req, actor).await
    }
    async fn save_address_arrangement(
        &self,
        id: &str,
        delivery_scope: bool,
        req: SaveAddressArrangement,
        actor: &AuditActor,
    ) -> Result<()> {
        let process = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let sales_id = if delivery_scope {
                        context::authorize_delivery(&process, &id, &actor, executor)
                            .await?
                            .sales_order_id
                            .to_string()
                    } else {
                        context::authorize_sales(&process, &id, &actor, true, executor).await?;
                        id.clone()
                    };
                    let mut context = Context::load(&process, &sales_id, executor).await?;
                    context.load_snapshots(&process, executor).await?;
                    save(&process, &mut context, delivery_scope.then_some(id.as_str()), req, &actor, executor)
                        .await
                })
            })
            .await
    }
}
async fn save(
    process: &FulfillmentProcess,
    context: &mut Context,
    target: Option<&str>,
    mut req: SaveAddressArrangement,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if req.expected_version != context.version(target) {
        return Err(Error::ConflictError("收货安排或发货进度已变化，请刷新后重试".into()));
    }
    let capacities =
        if let Some(id) = target { context.task_plan(id)?.0 } else { context.capacities.clone() };
    validate_arrangement(&mut req.lines, &capacities)?;
    let previous = if let Some(id) = target { context.task_plan(id)?.1 } else { context.plan.clone() };
    if requires_change_reason(&previous, &req.lines)? {
        req.reason = normalize_required_text(req.reason, "修改已有收货安排请填写原因", 500, "修改原因过长")?;
    } else if req.reason.chars().count() > 500 {
        return Err(Error::ValidationError("修改原因过长".into()));
    }
    let mut plan = merge_plan(context, target, &req.lines)?;
    validate_arrangement(&mut plan, &context.capacities)?;
    let snapshots = assignments(process, context, target, &plan, &req.lines)?;
    persist_drafts(process, context, snapshots, req.reset_packages, executor).await?;
    let content_encrypted = process
        .sensitive_data
        .encrypt(&serde_json::to_string(&plan).map_err(|_| Error::Internal("收货安排无法保存".into()))?)?;
    let record = AddressArrangement {
        base: context
            .stored
            .as_ref()
            .map(|record| record.base.clone())
            .unwrap_or_else(|| BaseModel::new(context.sales.base.id.clone())),
        source_revision: context.revision.clone(),
        content_encrypted,
        updated_by: actor.id().into(),
        reason: req.reason,
    };
    persist_record(process, context, record, actor, executor).await
}
fn merge_plan(
    context: &Context,
    target: Option<&str>,
    input: &[ArrangementLine],
) -> Result<Vec<ArrangementLine>> {
    let Some(id) = target else {
        return Ok(input.to_vec());
    };
    let mut pool = AddressPool(context.plan.clone());
    if let Some((_, snapshot)) = context.snapshots.iter().find(|(key, _)| key == id) {
        pool.consume(snapshot, &context.task_lines(id))?;
    }
    for row in input {
        let line = context
            .lines
            .iter()
            .find(|line| line.base.id == row.line_id && line.delivery_id.as_ref() == id)
            .ok_or_else(|| Error::ValidationError("商品不属于当前任务".into()))?;
        let plan = pool
            .0
            .iter_mut()
            .find(|plan| plan.line_id == line.sales_order_line_id.as_ref())
            .ok_or_else(|| Error::ConflictError("销售商品已变化".into()))?;
        plan.allocations.retain(|allocation| allocation.quantity.to_decimal() > Default::default());
        plan.allocations.extend(row.allocations.clone());
    }
    Ok(pool.0)
}
fn assignments(
    process: &FulfillmentProcess,
    context: &Context,
    target: Option<&str>,
    plan: &[ArrangementLine],
    input: &[ArrangementLine],
) -> Result<Vec<(String, DeliveryAddressSnapshot)>> {
    if context.sources.as_ref().is_some_and(|sources| !sources.external.is_empty()) {
        return Err(Error::BusinessLogicError(
            "该销售单已有供应商接口订单，收货地址需按供应商订单变更流程核对".into(),
        ));
    }
    let mut pool = AddressPool(plan.to_vec());
    for delivery in &context.deliveries {
        if !delivery.is_editable() || target.is_some_and(|id| id != delivery.base.id) {
            let snapshot =
                context.snapshots.iter().find(|(id, _)| *id == delivery.base.id).ok_or_else(|| {
                    Error::ConflictError("历史发货缺少可核对的地址，请先核对原发货记录".into())
                })?;
            pool.consume(&snapshot.1, &context.task_lines(&delivery.base.id))?;
        }
    }
    if target.is_none() {
        return reconcile_drafts(process, context, pool);
    }
    let mut snapshots = vec![];
    for delivery in context.deliveries.iter().filter(|row| row.is_editable()) {
        let version = context.source_version(delivery)?;
        let lines = context.task_lines(&delivery.base.id);
        let snapshot = if target.is_some_and(|id| id != delivery.base.id) {
            context
                .snapshots
                .iter()
                .find(|(id, _)| *id == delivery.base.id)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| Error::ConflictError("其他任务地址无法核对".into()))?
        } else if target.is_some() {
            let scoped = lines
                .into_iter()
                .map(|mut line| {
                    line.sales_order_line_id = line.base.id.clone().into();
                    line
                })
                .collect::<Vec<_>>();
            AddressPool(input.to_vec()).assign(&scoped, version, &process.fingerprint_key)?
        } else {
            pool.assign(&lines, version, &process.fingerprint_key)?
        };
        snapshots.push((delivery.base.id.clone(), snapshot));
    }
    Ok(snapshots)
}
async fn persist_drafts(
    process: &FulfillmentProcess,
    context: &mut Context,
    snapshots: Vec<(String, DeliveryAddressSnapshot)>,
    reset_packages: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    for (id, snapshot) in snapshots {
        let old = context.snapshots.iter().find(|(key, _)| key == &id).map(|(_, value)| value);
        let delivery = context
            .deliveries
            .iter_mut()
            .find(|row| row.base.id == id)
            .ok_or_else(|| Error::ConflictError("发货任务已变化".into()))?;
        let changed = old.is_none_or(|old| !old.same_arrangement(&snapshot));
        if !changed && delivery.packing_addresses_encrypted.is_some() {
            continue;
        }
        if changed {
            if let (Some(packing), Some(previous)) = (&delivery.packing, old) {
                let (rebased, removed) = packing.rebase(previous, &snapshot);
                if removed > 0 && !reset_packages {
                    return Err(Error::ConflictError(format!(
                        "本次调整影响 {} 的 {} 个包裹，请确认重新核对后保存",
                        delivery.delivery_no, removed
                    )));
                }
                let lines = context
                    .lines
                    .iter()
                    .filter(|line| line.delivery_id.as_ref() == id)
                    .cloned()
                    .collect::<Vec<_>>();
                rebased.validate(&snapshot, &lines, false)?;
                delivery.packing = Some(rebased);
            } else if delivery.packing.is_some() {
                return Err(Error::ConflictError("原包裹缺少可信地址，请先核对原发货记录".into()));
            }
        }
        delivery_addresses::freeze_addresses(&process.sensitive_data, delivery, &snapshot)?;
        process.db.deliveries().update(delivery, executor).await?;
        let mut task = task::load_single_open_task(&process.db, "delivery", &id, executor).await?;
        task::ensure_task_matches_frozen_identity(&task, &task::FulfillmentTaskObject::Delivery(delivery))?;
        task.subject_version = delivery.base.version.to_string();
        process.db.work_items().update(&mut task, executor).await?;
    }
    Ok(())
}
async fn persist_record(
    process: &FulfillmentProcess,
    context: &mut Context,
    mut record: AddressArrangement,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    // 与发货确认共用销售主表写锁，阻止地址修改与确认分别提交。
    process.db.sales_orders().update(&mut context.sales, executor).await?;
    if context.stored.is_some() {
        process.db.address_arrangements().update(&mut record, executor).await?;
    } else {
        process.db.address_arrangements().create(&record, executor).await?;
    }
    let mut history = record.clone();
    history.base = BaseModel::new(format!("history:{}:{}", context.sales.base.id, record.base.version));
    process.db.address_arrangements().create(&history, executor).await?;
    let audit = actor.clone().resource_log(
        "sales_order.delivery_arrangement",
        "sales_order",
        context.sales.base.id.clone(),
    )?;
    persist_log(&process.db, &audit, executor).await?;
    Ok(())
}

fn reconcile_drafts(
    process: &FulfillmentProcess,
    context: &Context,
    mut pool: AddressPool,
) -> Result<Vec<(String, DeliveryAddressSnapshot)>> {
    let mut ids = vec![];
    let mut tasks = vec![];
    for delivery in context.deliveries.iter().filter(|row| row.is_editable()) {
        let version = context.source_version(delivery)?;
        let mut snapshot = context
            .snapshots
            .iter()
            .find(|(id, _)| *id == delivery.base.id)
            .map(|(_, snapshot)| snapshot.clone())
            .unwrap_or_else(|| DeliveryAddressSnapshot {
                source_version: version.clone(),
                destinations: vec![],
            });
        snapshot.source_version = version;
        ids.push(delivery.base.id.clone());
        tasks.push((snapshot, context.task_lines(&delivery.base.id)));
    }
    Ok(ids.into_iter().zip(pool.reconcile(&tasks, &process.fingerprint_key)?).collect())
}
