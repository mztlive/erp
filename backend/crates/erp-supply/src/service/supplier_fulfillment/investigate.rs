//! 调查意图、冻结结果与终态证据的唯一领域规则。
use erp_core::ids::SupplierOrderActionId;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use crate::dto::supplier_fulfillment::*;
use crate::entity::supplier_api::SupplierApiCapabilityCode;
use crate::entity::supplier_fulfillment::*;
use crate::ports::supplier_gateway::{DispatchOutcome, InvestigationOutcome};
use crate::repository::SupplierFulfillmentExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
const INVESTIGATION_EVIDENCE_SCHEMA: &str = "W26_INVESTIGATION_V1";
const INVESTIGATION_INTENT_SCHEMA: &str = "W26_INVESTIGATION_INTENT_V1";
const INVESTIGATION_PREPARED_SCHEMA: &str = "W26_INVESTIGATION_PREPARED_V1";
/// 调查本域消费的原订单/动作身份；正式任务由外层单独持有。
#[derive(Debug, Clone)]
pub struct InvestigationSubject {
    pub order_id: String,
    pub expected_order_version: u64,
    pub action: SupplierOrderInvestigationAction,
    pub operation_id: String,
    pub target_action_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "result", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreparedInvestigation {
    PersistedTerminal(SupplierOrderResolution),
    Queried(InvestigationOutcome),
    Replayed(DispatchOutcome),
}

#[derive(Debug, Clone)]
pub struct InvestigationFinding {
    pub outcome: SupplierOrderInvestigationOutcome,
    pub resolution: Option<SupplierOrderResolution>,
    pub summary: String,
}

/// 已持久化的结构化调查证据；详情投影与命令验证复用同一解析结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestigationEvidenceRecord {
    pub(super) schema: String,
    pub(super) action: SupplierOrderInvestigationAction,
    /// 原供应商动作身份。
    pub(super) target_supplier_action_id: String,
    /// 已经记录的调查结果。
    pub(super) outcome: SupplierOrderInvestigationOutcome,
    /// 证据已验证的业务终态。
    pub(super) verified_resolution: Option<SupplierOrderResolution>,
    pub(super) operation_id: String,
    /// 适于详情展示的调查摘要。
    pub(super) summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct InvestigationIntentRecord {
    schema: String,
    action: SupplierOrderInvestigationAction,
    target_supplier_action_id: String,
    operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DurablePreparedInvestigation {
    schema: String,
    action: SupplierOrderInvestigationAction,
    target_supplier_action_id: String,
    operation_id: String,
    prepared: PreparedInvestigation,
}
impl InvestigationEvidenceRecord {
    /// 返回证据所绑定的原供应商动作身份。
    pub fn target_supplier_action_id(&self) -> &str {
        &self.target_supplier_action_id
    }

    /// 返回已经记录的调查结果，供详情只读投影。
    pub fn outcome(&self) -> SupplierOrderInvestigationOutcome {
        self.outcome
    }

    /// 返回证据已经验证的业务终态，供详情只读投影。
    pub fn verified_resolution(&self) -> Option<SupplierOrderResolution> {
        self.verified_resolution
    }

    /// 返回已经记录的调查摘要，供详情只读展示。
    pub fn summary(&self) -> &str {
        &self.summary
    }
    /// 返回既有命令操作身份，供上层结果投影。
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
}

/// 返回原供应商动作需要的连接能力；查询记录不能作为再次提交目标。
pub fn capability_for_action(action_type: SupplierOrderActionType) -> Result<SupplierApiCapabilityCode> {
    match action_type {
        SupplierOrderActionType::Place => Ok(SupplierApiCapabilityCode::Order),
        SupplierOrderActionType::Cancel => Ok(SupplierApiCapabilityCode::Cancel),
        SupplierOrderActionType::Refund => Ok(SupplierApiCapabilityCode::Refund),
        SupplierOrderActionType::Query => {
            Err(Error::BusinessLogicError("查询记录不能作为再次提交的目标".to_string()))
        },
    }
}

/// 校验原下单可重放且最新调查证据明确证明尚未形成结果。
pub async fn ensure_replay_safe(
    db: &Database,
    order: &SupplierFulfillmentOrder,
    target_action: &SupplierOrderAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !order.can_replay_place_action(target_action) {
        return Err(Error::BusinessLogicError(
            "仅结果未知的原下单请求可以在明确无结果后再次提交".to_string(),
        ));
    }
    let actions = db
        .supplier_order_actions()
        .list_by_order_and_type_newest(
            &SupplierFulfillmentOrderId::new(order.base.id.as_str()),
            SupplierOrderActionType::Query,
            executor,
        )
        .await?;
    let latest = actions.into_iter().find_map(|action| {
        let record = parse_investigation_evidence(&action).ok()?;
        (record.target_supplier_action_id == target_action.base.id).then_some(record)
    });
    if latest.is_some_and(|record| record.outcome == SupplierOrderInvestigationOutcome::VerifiedNoResult) {
        return Ok(());
    }
    Err(Error::BusinessLogicError("尚无最新的明确无结果证据，禁止再次提交供应商下单".to_string()))
}

fn investigation_intent_record(context: &InvestigationSubject) -> InvestigationIntentRecord {
    InvestigationIntentRecord {
        schema: INVESTIGATION_INTENT_SCHEMA.to_string(),
        action: context.action,
        target_supplier_action_id: context.target_action_id.clone(),
        operation_id: context.operation_id.clone(),
    }
}

pub fn bounded_prepared_investigation(prepared: PreparedInvestigation) -> PreparedInvestigation {
    match prepared {
        PreparedInvestigation::Queried(InvestigationOutcome::VerifiedNoResult { summary }) => {
            PreparedInvestigation::Queried(InvestigationOutcome::VerifiedNoResult {
                summary: bounded_summary(&summary),
            })
        },
        PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown { summary }) => {
            PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown {
                summary: bounded_summary(&summary),
            })
        },
        PreparedInvestigation::Replayed(DispatchOutcome::Rejected { summary }) => {
            PreparedInvestigation::Replayed(DispatchOutcome::Rejected { summary: bounded_summary(&summary) })
        },
        PreparedInvestigation::Replayed(DispatchOutcome::ResultUnknown { summary }) => {
            PreparedInvestigation::Replayed(DispatchOutcome::ResultUnknown {
                summary: bounded_summary(&summary),
            })
        },
        PreparedInvestigation::Replayed(DispatchOutcome::Failed { error_class, summary }) => {
            PreparedInvestigation::Replayed(DispatchOutcome::Failed {
                error_class,
                summary: bounded_summary(&summary),
            })
        },
        other => other,
    }
}

pub fn validate_investigation_intent(
    evidence: &SupplierOrderAction,
    context: &InvestigationSubject,
    expected_idempotency_key: &str,
) -> Result<()> {
    if evidence.supplier_fulfillment_order_id.as_ref() != context.order_id
        || evidence.action_type != SupplierOrderActionType::Query
        || evidence.idempotency_key != expected_idempotency_key
    {
        return Err(Error::ConflictError("调查意图身份与当前命令不一致".to_string()));
    }
    let intent: InvestigationIntentRecord = serde_json::from_str(
        evidence.request_summary.as_deref().ok_or_else(|| Error::Internal("调查意图摘要为空".to_string()))?,
    )
    .map_err(|_| Error::Internal("调查意图摘要格式无效".to_string()))?;
    if intent != investigation_intent_record(context) {
        return Err(Error::ConflictError("调查意图已用于不同的命令载荷".to_string()));
    }
    Ok(())
}

pub fn parse_prepared_investigation(
    evidence: &SupplierOrderAction,
    context: &InvestigationSubject,
) -> Result<PreparedInvestigation> {
    let durable: DurablePreparedInvestigation = serde_json::from_str(
        evidence
            .response_summary
            .as_deref()
            .ok_or_else(|| Error::Internal("调查网关结果尚未持久化".to_string()))?,
    )
    .map_err(|_| Error::ConflictError("调查结果已进入领域结算，禁止重复外调".to_string()))?;
    if durable.schema != INVESTIGATION_PREPARED_SCHEMA
        || durable.action != context.action
        || durable.target_supplier_action_id != context.target_action_id
        || durable.operation_id != context.operation_id
    {
        return Err(Error::ConflictError("已持久化调查结果与当前命令不一致".to_string()));
    }
    Ok(durable.prepared)
}

pub fn apply_prepared_investigation(
    context: &InvestigationSubject,
    prepared: &PreparedInvestigation,
    order: &mut SupplierFulfillmentOrder,
    target_action: &mut SupplierOrderAction,
) -> Result<InvestigationFinding> {
    if context.action == SupplierOrderInvestigationAction::QueryResult
        && let Some(resolution) = order.verified_resolution(target_action).map(Into::into)
    {
        return Ok(InvestigationFinding {
            outcome: SupplierOrderInvestigationOutcome::VerifiedTerminal,
            resolution: Some(resolution),
            summary: format!("已由当前供应商业务事实核实结果：{}", resolution.label()),
        });
    }
    match prepared {
        PreparedInvestigation::PersistedTerminal(resolution) => {
            if order.verified_resolution(target_action).map(Into::into) != Some(*resolution) {
                return Err(Error::ConflictError("供应商业务结果已变化，请刷新后重试".to_string()));
            }
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedTerminal,
                resolution: Some(*resolution),
                summary: format!("已由当前供应商业务事实核实结果：{}", resolution.label()),
            })
        },
        PreparedInvestigation::Queried(InvestigationOutcome::VerifiedNoResult { summary }) => {
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedNoResult,
                resolution: None,
                summary: summary.clone(),
            })
        },
        PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown { summary }) => {
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::ResultUnknown,
                resolution: None,
                summary: summary.clone(),
            })
        },
        PreparedInvestigation::Replayed(outcome) => {
            apply_replay_outcome(order, target_action, outcome.clone())
        },
    }
}

pub fn apply_replay_outcome(
    order: &mut SupplierFulfillmentOrder,
    target_action: &mut SupplierOrderAction,
    outcome: DispatchOutcome,
) -> Result<InvestigationFinding> {
    match outcome {
        DispatchOutcome::Succeeded { external_request_id, external_order_no: Some(external_order_no) } => {
            order.update(SupplierFulfillmentOrderUpdate { external_order_no: Some(external_order_no) })?;
            order.advance_fulfillment(FulfillmentStatus::Accepted)?;
            target_action.update(SupplierOrderActionUpdate {
                status: Some(SupplierOrderActionStatus::Succeeded),
                external_request_id: Some(external_request_id),
                response_summary: Some("按原请求再次提交后，供应商已明确接单".to_string()),
                next_attempt_at: None,
                ..Default::default()
            })?;
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedTerminal,
                resolution: Some(SupplierOrderResolution::OrderAccepted),
                summary: "已按原请求安全再次提交，并取得明确接单结果".to_string(),
            })
        },
        DispatchOutcome::Succeeded { external_order_no: None, .. } => {
            target_action.update(SupplierOrderActionUpdate {
                status: Some(SupplierOrderActionStatus::ResultUnknown),
                response_summary: Some("再次提交的响应缺少供应商订单号，结果仍未知".to_string()),
                ..Default::default()
            })?;
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::ResultUnknown,
                resolution: None,
                summary: "再次提交的响应不足以证明供应商业务结果".to_string(),
            })
        },
        DispatchOutcome::Rejected { summary } => {
            order.advance_fulfillment(FulfillmentStatus::Rejected)?;
            target_action.update(SupplierOrderActionUpdate {
                status: Some(SupplierOrderActionStatus::Failed),
                response_summary: Some(summary.clone()),
                ..Default::default()
            })?;
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedTerminal,
                resolution: Some(SupplierOrderResolution::OrderRejected),
                summary,
            })
        },
        DispatchOutcome::ResultUnknown { summary } => {
            target_action.update(SupplierOrderActionUpdate {
                status: Some(SupplierOrderActionStatus::ResultUnknown),
                response_summary: Some(summary.clone()),
                ..Default::default()
            })?;
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::ResultUnknown,
                resolution: None,
                summary,
            })
        },
        DispatchOutcome::Failed { summary, .. } => {
            target_action.record_attempt(None);
            target_action.update(SupplierOrderActionUpdate {
                status: Some(SupplierOrderActionStatus::ResultUnknown),
                response_summary: Some(summary.clone()),
                ..Default::default()
            })?;
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::ResultUnknown,
                resolution: None,
                summary,
            })
        },
    }
}

pub fn evidence_action_status(outcome: SupplierOrderInvestigationOutcome) -> SupplierOrderActionStatus {
    match outcome {
        SupplierOrderInvestigationOutcome::VerifiedTerminal
        | SupplierOrderInvestigationOutcome::VerifiedNoResult => SupplierOrderActionStatus::Succeeded,
        SupplierOrderInvestigationOutcome::ResultUnknown => SupplierOrderActionStatus::ResultUnknown,
    }
}

/// 校验调查证据属于当前订单并已证明所选业务终态。
pub fn verified_terminal_evidence(
    evidence: &SupplierOrderAction,
    order: &SupplierFulfillmentOrder,
    expected_resolution: SupplierOrderResolution,
) -> Result<InvestigationEvidenceRecord> {
    if evidence.supplier_fulfillment_order_id.as_ref() != order.base.id
        || evidence.action_type != SupplierOrderActionType::Query
        || evidence.status != SupplierOrderActionStatus::Succeeded
    {
        return Err(Error::BusinessLogicError("结果证据不属于当前供应商履约订单".to_string()));
    }
    let record = parse_investigation_evidence(evidence)?;
    if record.outcome != SupplierOrderInvestigationOutcome::VerifiedTerminal
        || record.verified_resolution != Some(expected_resolution)
    {
        return Err(Error::BusinessLogicError("供应商证据尚未证明所选业务结果".to_string()));
    }
    Ok(record)
}

/// 解析持久化调查结果并校验已登记的证据结构版本。
pub fn parse_investigation_evidence(action: &SupplierOrderAction) -> Result<InvestigationEvidenceRecord> {
    let summary = action
        .response_summary
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("供应商调查证据缺少结构化结果".to_string()))?;
    let record: InvestigationEvidenceRecord = serde_json::from_str(summary)
        .map_err(|_| Error::BusinessLogicError("供应商调查证据格式非法".to_string()))?;
    if record.schema != INVESTIGATION_EVIDENCE_SCHEMA {
        return Err(Error::BusinessLogicError("供应商调查证据版本未注册".to_string()));
    }
    Ok(record)
}

pub fn bounded_summary(value: &str) -> String {
    value.chars().take(512).collect()
}
/// 构造原结构化结果；字段写权限仍只在领域证据模块。
pub fn investigation_evidence_record(
    subject: &InvestigationSubject,
    target_supplier_action_id: String,
    finding: &InvestigationFinding,
) -> InvestigationEvidenceRecord {
    InvestigationEvidenceRecord {
        schema: INVESTIGATION_EVIDENCE_SCHEMA.to_string(),
        action: subject.action,
        target_supplier_action_id,
        outcome: finding.outcome,
        verified_resolution: finding.resolution,
        operation_id: subject.operation_id.clone(),
        summary: bounded_summary(&finding.summary),
    }
}
/// 原意图序列化和实体构造；外层事务决定写入时点。
pub fn prepare_intent(
    subject: &InvestigationSubject,
    evidence_id: String,
    idempotency_key: String,
    order_id: &str,
) -> Result<SupplierOrderAction> {
    let intent_summary = serde_json::to_string(&investigation_intent_record(subject))
        .map_err(|error| Error::Internal(format!("调查意图序列化失败: {error}")))?;
    Ok(SupplierOrderAction::new(
        SupplierOrderActionId::new(evidence_id),
        SupplierOrderActionData::query_intent(
            SupplierFulfillmentOrderId::new(order_id),
            idempotency_key,
            intent_summary,
        ),
    )?)
}
/// 在原已有意图上冻结一次网关结果。
pub fn prepare_durable_evidence(
    evidence: &mut SupplierOrderAction,
    subject: &InvestigationSubject,
    prepared: &PreparedInvestigation,
) -> Result<()> {
    let durable = DurablePreparedInvestigation {
        schema: INVESTIGATION_PREPARED_SCHEMA.to_string(),
        action: subject.action,
        target_supplier_action_id: subject.target_action_id.clone(),
        operation_id: subject.operation_id.clone(),
        prepared: prepared.clone(),
    };
    let summary = serde_json::to_string(&durable)
        .map_err(|error| Error::Internal(format!("调查结果序列化失败: {error}")))?;
    evidence.update(SupplierOrderActionUpdate {
        status: Some(SupplierOrderActionStatus::Pending),
        response_summary: Some(summary),
        next_attempt_at: None,
        ..Default::default()
    })?;
    Ok(())
}
/// 在外层唯一执行器中保存原动作变更。
pub async fn persist_action(
    db: &Database,
    action: &mut SupplierOrderAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_order_actions().update(action, executor).await?;
    Ok(())
}
/// 在外层唯一执行器中追加原动作事实。
pub async fn create_action(
    db: &Database,
    action: &SupplierOrderAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_order_actions().create(action, executor).await?;
    Ok(())
}
/// 在外层唯一执行器保存原订单变化，不新增状态判断。
pub async fn persist_order(
    db: &Database,
    order: &mut SupplierFulfillmentOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_fulfillment_orders().update(order, executor).await?;
    Ok(())
}

/// 已核验 durable 结果一致后推进正式证据，保留原状态及摘要字段。
pub fn finish_evidence(
    evidence: &mut SupplierOrderAction,
    outcome: SupplierOrderInvestigationOutcome,
    response_summary: String,
) -> Result<()> {
    evidence.update(SupplierOrderActionUpdate {
        status: Some(evidence_action_status(outcome)),
        response_summary: Some(response_summary),
        next_attempt_at: None,
        ..Default::default()
    })?;
    Ok(())
}

#[cfg(test)]
mod investigation_tests {
    use erp_core::ids::{SupplierFulfillmentOrderId, SupplierOrderActionId};

    use super::{
        DurablePreparedInvestigation, INVESTIGATION_PREPARED_SCHEMA, InvestigationSubject,
        PreparedInvestigation, investigation_intent_record, parse_prepared_investigation,
        validate_investigation_intent,
    };
    use crate::dto::supplier_fulfillment::SupplierOrderInvestigationAction;
    use crate::entity::supplier_fulfillment::{
        SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionStatus, SupplierOrderActionType,
    };
    use crate::ports::supplier_gateway::InvestigationOutcome;

    fn context() -> InvestigationSubject {
        InvestigationSubject {
            order_id: "order-1".to_string(),
            expected_order_version: 3,
            action: SupplierOrderInvestigationAction::QueryResult,
            operation_id: "operation-1".to_string(),
            target_action_id: "target-action-1".to_string(),
        }
    }

    #[test]
    fn durable_intent_freezes_gateway_result_before_domain_reconciliation() {
        let context = context();
        let prepared = PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown {
            summary: "供应商暂未给出终态".to_string(),
        });
        let durable = DurablePreparedInvestigation {
            schema: INVESTIGATION_PREPARED_SCHEMA.to_string(),
            action: context.action,
            target_supplier_action_id: context.target_action_id.clone(),
            operation_id: context.operation_id.clone(),
            prepared: prepared.clone(),
        };
        let evidence = SupplierOrderAction::new(
            SupplierOrderActionId::new("evidence-1"),
            SupplierOrderActionData {
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(&context.order_id),
                action_type: SupplierOrderActionType::Query,
                idempotency_key: "stable-key".to_string(),
                status: SupplierOrderActionStatus::Pending,
                external_request_id: None,
                request_summary: Some(serde_json::to_string(&investigation_intent_record(&context)).unwrap()),
                response_summary: Some(serde_json::to_string(&durable).unwrap()),
                attempt_count: 1,
                next_attempt_at: None,
            },
        )
        .unwrap();

        validate_investigation_intent(&evidence, &context, "stable-key").unwrap();
        assert_eq!(parse_prepared_investigation(&evidence, &context).unwrap(), prepared);

        let mut changed = context;
        changed.operation_id = "operation-2".to_string();
        assert!(validate_investigation_intent(&evidence, &changed, "stable-key").is_err());
    }
}
