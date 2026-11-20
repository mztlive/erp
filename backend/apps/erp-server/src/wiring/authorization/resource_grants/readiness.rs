//! 按当前业务事实查询读取授权的生成状态。
use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::Result;
use erp_identity::entity::resource_access::{GrantRule, SourceKey};
use erp_identity::ports::ResourceGrantFacts;
use erp_identity::service::resource_access::GrantEventService;
use mongodb::Database;
use persistence_core::Executor;

use super::facts::Facts;

/// 检查账号的首次开票审批与后续转派来源，任一已核实来源待生成才返回 `true`。
/// 复用调用方的执行器，事实错配和读取失败保持原错误。
pub(in crate::wiring) async fn invoice_pending(
    db: &Database,
    request_id: &str,
    recipient: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let initial = GrantRule::InvoiceApproved.source_key(request_id, recipient);
    if source_readiness(db, &initial, executor).await? == GrantReadiness::NotReady {
        return Ok(true);
    }

    let Some(request) = db.sales_invoice_requests().find_by_id(request_id, executor).await? else {
        return Ok(false);
    };
    let Some(task_id) = request.work_item_id else {
        return Ok(false);
    };
    let reassigned = GrantRule::InvoiceTaskReassigned.source_key(task_id, recipient);
    Ok(source_readiness(db, &reassigned, executor).await? == GrantReadiness::NotReady)
}

/// 仅表示读取授权生成状态，不授予功能权限或办理资格。
#[derive(Debug, PartialEq, Eq)]
pub(in crate::wiring) enum GrantReadiness {
    Absent,
    NotReady,
    Ready,
    Terminated,
}

/// 按服务端确定的来源查找正式事件，并用同一执行器重算目标，与当前来源版本和目标对比，区分不存在、待生成、已就绪和已终止。
/// 没有正式事件返回 `Absent`；账号失效、关联损坏或读取失败不得当作待生成，也不能以就绪结果替代访问授权。
pub(in crate::wiring) async fn source_readiness(
    db: &Database,
    source: &SourceKey,
    executor: &mut dyn Executor,
) -> Result<GrantReadiness> {
    let queue = GrantEventService::new(db.clone());
    let Some(expected) = queue.source_event(source, executor).await? else {
        return Ok(GrantReadiness::Absent);
    };
    let Some(event) = queue.event(&expected, executor).await? else {
        return Ok(GrantReadiness::Absent);
    };

    let update = Facts { db: db.clone() }.compute_source(&event, executor).await?;
    let source = queue.source(&event.source, executor).await?;
    Ok(match source {
        Some(s) if s.terminated => GrantReadiness::Terminated,
        Some(s)
            if s.fact_version == update.fact_version
                && s.rule_version == update.rule_version
                && s.targets == update.targets =>
        {
            GrantReadiness::Ready
        },
        _ => GrantReadiness::NotReady,
    })
}
