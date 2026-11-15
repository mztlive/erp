//! 查询原结果仅唤醒既有调查与跟踪，不更换单号或重新发送写动作。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_permissions::supplier_fulfillment;
use erp_supply::api_order::ApiOrderExt;
use erp_supply::order_tracking::TrackingState;
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::service::supplier_fulfillment::FulfillmentOrderAccess;
use persistence_core::Transactional;
use serde::Deserialize;

use super::{ExecutionSetupView, SupplierApiOrderProcess};
use crate::{Error, Result};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionQueryRequest {
    pub expected_version: u64,
}

impl SupplierApiOrderProcess {
    /// 有权限的经办人请求调查原外部动作，绝不重新创建或确认。
    /// # 参数
    /// `id` 和预期版本指向原执行；调用人及对象授权在事务内重新校验。
    /// # 返回
    /// 唤醒原调查和订单跟踪后的当前资料。
    /// # 错误
    /// 越权、版本变化、没有原请求证据或数据库提交未知时拒绝。
    pub async fn query_execution_result(
        &self,
        id: &str,
        request: ExecutionQueryRequest,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<ExecutionSetupView> {
        let (db, id, actor, access) = (self.db.clone(), id.to_owned(), actor.clone(), access.clone());
        let result_id = id.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require_order(&actor, &supplier_fulfillment::ORDER_SUBMIT, &id, executor).await?;
                    let mut execution = db
                        .supplier_api_order_executions()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("原接口执行不存在".into()))?;
                    if request.expected_version != execution.base.version
                        || !execution.original_result_query_allowed()
                    {
                        return Err(Error::ConflictError(
                            "执行已变化或没有待查询的原请求，请刷新后核对".into(),
                        ));
                    }
                    execution.wake_original_result_query(Instant::now())?;
                    execution.wake_receipt_confirmation(Instant::now());
                    db.supplier_api_order_executions().update(&mut execution, executor).await?;
                    if let Some(mut tracking) = db.supplier_order_tracking().find_by_id(&id, executor).await?
                    {
                        tracking.poll_after = Instant::now();
                        tracking.state = TrackingState::Polling;
                        db.supplier_order_tracking().update(&mut tracking, executor).await?;
                    }
                    let audit = actor.resource_log(
                        "supplier_fulfillment.query_original_result",
                        "supplier_fulfillment_order",
                        id,
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await?;
        self.execution_setup(&result_id).await
    }
}
