use std::cmp::Ordering;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_supply::api_order::ApiOrderExt;
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::order_tracking::{OrderObservation, OrderTracking, TrackingState};
use erp_supply::repository::SupplierFulfillmentExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::{SupplierOrderTrackingProcess, later};
use crate::audit::persist_log;
use crate::{Error, Result};

impl SupplierOrderTrackingProcess {
    /// 在事务中取得单个订单的有期查询权。
    /// # 参数
    /// `id` 为已经登记的跟踪身份。
    /// # 返回
    /// 到期且没有活跃查询时返回当前检查点，否则返回 None。
    /// # 错误
    /// 检查点缺失或并发写入失败时拒绝。
    pub(super) async fn claim(&self, id: &str) -> Result<Option<OrderTracking>> {
        let (db, id) = (self.db.clone(), id.to_owned());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut tracking = db
                        .supplier_order_tracking()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("订单跟踪记录不存在".into()))?;
                    let now = Instant::now();
                    if is_superseded(&db, &id, executor).await? {
                        tracking.stop_for_purchase_change();
                        db.supplier_order_tracking().update(&mut tracking, executor).await?;
                        return Ok(None);
                    }
                    if tracking.state == TrackingState::Complete
                        || tracking.poll_after > now
                        || tracking.lease_until.is_some_and(|until| until > now)
                    {
                        return Ok(None);
                    }
                    tracking.lease_id = Some(next_id());
                    tracking.lease_until = Some(later(300));
                    db.supplier_order_tracking().update(&mut tracking, executor).await?;
                    Ok(Some(tracking))
                })
            })
            .await
    }

    /// 保存来源证据后再尝试原子应用，冲突和失败也保留真实来源观察。
    /// # 参数
    /// 已取得租约的跟踪记录和已核验身份的查询结果。
    /// # 返回
    /// 已成为当前应用目标的观察号。
    /// # 错误
    /// 租约失效、来源版本冲突、业务状态冲突或保存失败时拒绝应用。
    pub(super) async fn persist_observation(
        &self,
        claimed: &OrderTracking,
        observation: OrderObservation,
    ) -> Result<String> {
        let previous = if let Some(id) = &claimed.latest_observation_id {
            self.db.supplier_order_observations().find_by_id(id, &mut NoTransaction).await?
        } else {
            None
        };
        let id = if previous.as_ref().is_some_and(|previous| observation.same_content(previous)) {
            previous.expect("已证明存在前一份观察").base.id
        } else {
            self.db.supplier_order_observations().create(&observation, &mut NoTransaction).await?;
            observation.base.id.clone()
        };
        let (db, claimed) = (self.db.clone(), claimed.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut current = load_claimed(&db, &claimed, executor).await?;
                    let previous = if let Some(id) = &current.latest_observation_id {
                        db.supplier_order_observations().find_by_id(id, executor).await?
                    } else {
                        None
                    };
                    ensure_source_progress(previous.as_ref(), &observation)?;
                    apply_supplier_state(&db, &current.base.id, &observation, executor).await?;
                    current.latest_observation_id = Some(id.clone());
                    current.last_verified_at = Some(observation.observed_at);
                    db.supplier_order_tracking().update(&mut current, executor).await?;
                    Ok(id)
                })
            })
            .await
    }

    /// 释放尚未取得外部单号的检查点，创建恢复由发单流程处理。
    /// # 参数
    /// `claimed` 为本轮取得的查询权。
    /// # 返回
    /// 将下次查询安排到一分钟后。
    /// # 错误
    /// 查询权已变化或保存失败时拒绝。
    pub(super) async fn release_uncreated(&self, claimed: &OrderTracking) -> Result<()> {
        self.release(claimed, None, false).await
    }

    /// 保存安全错误摘要并释放查询权，保持后续只读补查。
    /// # 参数
    /// `claimed` 为本轮查询权；`error` 为已脱敏的业务提示。
    /// # 返回
    /// 登记人工关注状态并按五分钟退避。
    /// # 错误
    /// 检查点已被接管或写入失败时拒绝。
    pub(super) async fn finish_failure(&self, claimed: &OrderTracking, error: &str) -> Result<()> {
        self.release(claimed, Some(error.chars().take(500).collect()), true).await
    }

    async fn release(&self, claimed: &OrderTracking, error: Option<String>, manual: bool) -> Result<()> {
        let (db, claimed) = (self.db.clone(), claimed.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if is_superseded(&db, &claimed.base.id, executor).await? {
                        return Ok(());
                    }
                    let mut current = load_claimed(&db, &claimed, executor).await?;
                    current.state = if manual { TrackingState::Manual } else { TrackingState::Polling };
                    current.last_error = error;
                    current.lease_id = None;
                    current.lease_until = None;
                    current.poll_after = later(if manual { 300 } else { 60 });
                    db.supplier_order_tracking().update(&mut current, executor).await?;
                    Ok(())
                })
            })
            .await
    }
}

/// 在写入事务内重验本轮查询权，禁止旧执行者覆盖新执行者。
/// # 参数
/// 数据库、本轮检查点及调用方事务执行器。
/// # 返回
/// 当前仍持有相同查询权的最新检查点。
/// # 错误
/// 检查点缺失或查询权变化时拒绝。
pub(super) async fn load_claimed(
    db: &Database,
    claimed: &OrderTracking,
    executor: &mut dyn Executor,
) -> Result<OrderTracking> {
    if is_superseded(db, &claimed.base.id, executor).await? {
        return Err(Error::ConflictError("原供应商执行已被采购变更替代，停止应用迟到结果".into()));
    }
    let current = db
        .supplier_order_tracking()
        .find_by_id(&claimed.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("订单跟踪记录不存在".into()))?;
    if current.lease_id != claimed.lease_id || current.lease_id.is_none() {
        return Err(Error::ConflictError("订单跟踪已由其他任务接管".into()));
    }
    Ok(current)
}

/// 读取同一执行身份的权威退役事实；缺少关联不冒充已退役。
/// # 参数
/// 当前业务库、原供应商执行身份和调用方执行器。
/// # 返回
/// 执行存在且携带替代采购版本时返回 true。
/// # 错误
/// 数据库读取失败时返回原错误。
pub(super) async fn is_superseded(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<bool> {
    Ok(db
        .supplier_api_order_executions()
        .find_by_id(id, executor)
        .await?
        .is_some_and(|execution| execution.superseded_by_purchase_revision_id.is_some()))
}

fn ensure_source_progress(previous: Option<&OrderObservation>, current: &OrderObservation) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    match current.source_revision.compare(&previous.source_revision) {
        Some(Ordering::Less) => Err(Error::ConflictError("供应商返回旧版本订单，请稍后补查".into())),
        Some(Ordering::Equal) if !current.same_content(previous) => {
            Err(Error::ConflictError("供应商同一订单版本内容冲突，请核对".into()))
        },
        _ => Ok(()),
    }
}

async fn apply_supplier_state(
    db: &Database,
    order_id: &str,
    observation: &OrderObservation,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut order = db
        .supplier_fulfillment_orders()
        .find_by_id(order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))?;
    let mut changed = false;
    if let Some(status) = observation.fulfillment {
        changed |= order.apply_observed_fulfillment(status, observation.source_updated_at)?;
    }
    if let Some(status) = observation.cancellation {
        changed |= order.apply_observed_cancellation(status)?;
    }
    if changed {
        db.supplier_fulfillment_orders().update(&mut order, executor).await?;
        let actor = AuditActor::new(
            "system:supplier-order-tracking".into(),
            "system:supplier-order-tracking".into(),
            AccountKind::Admin,
        );
        let audit = actor.resource_log(
            "supplier_fulfillment.observe",
            "supplier_fulfillment_order",
            order.base.id,
        )?;
        persist_log(db, &audit, executor).await?;
    }
    Ok(())
}
