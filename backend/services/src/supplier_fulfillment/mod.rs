//! 域 D32 `supplier_fulfillment` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 下单：`supplier_fulfillment_orders` + `supplier_fulfillment_items` +
//!   首个 `PLACE` 动作 + `inbox_message` + 审计同事务（§6.19）；
//! - 取消/退款：动作头 + 动作行 + 订单状态推进 + `inbox_message` 同事务；
//! - 拒单结果：订单状态 + 状态历史 + 动作结果 + 审计同事务（§6.19 回调幂等）；
//! - 退款成功结果：退款事实头 + 分配行 + 订单退款进度同事务（§8.4 第 5 条
//!   实体可判定部分，`create_refund_fact_with_allocations` 要求事务执行器）。
//!
//! 供应商 API 调用在事务之外完成（P3 §7）：先事务内落 `inbox_message`，事务外经
//! [`SupplierGateway`] 派发（超时/重试上限/错误分类由网关实现承担），结果经
//! `inbox_message` + `integration_error_task` 承接（D34 仓储）；需要人工处理时，
//! 同事务创建 W26 正式 `work_item` 与审计，失败降级为可观测错误并记录
//! `account` 上下文。
//!
//! 跨域协作只经 DatabaseExt 调对方域 Repository（P3 §2）：D25 `supplier_api`
//! （连接与能力）、D29 `mall_order`（商城订单与明细）、D24 `supplier_offering`
//! （供给修订）、D30 `mall_after_sales`（售后申请与行，§6.19 净余额校验）、
//! D34 `integration_ops`（inbox_message / integration_error_task）。
//!
//! 资金/状态机入口一律幂等（§6.19）：下单键为 `fulfillment_order_no`，
//! 取消/退款键为「ERP 供应商订单号 + 动作类型 + 商城售后请求 ID」（本域拼装），
//! 拒单键为 `(connection_id, external_event_id)`，退款结果键为
//! `(connection_id, external_refund_no, external_refund_version)`；
//! 重复提交只返回原结果，不产生第二条正式事实。

use std::collections::HashMap;
use std::sync::Arc;

use database::{
    AccessControlExt, Executor, IntegrationOpsExt, MallAfterSalesExt, MallOrderExt, NoTransaction,
    SupplierApiExt, SupplierExt, SupplierFulfillmentExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, SourceSystemId, SupplierOrderActionId, SupplierOrderActionLineId,
    SupplierOrderStatusHistoryId, SupplierRefundAllocationId, SupplierRefundFactId, WorkItemId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, IntegrationErrorTaskId, MessageType,
};
use entities::money::{Amount, Quantity};
use entities::supplier_api::{SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiConnection};
use entities::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentItem, SupplierFulfillmentItemData,
    SupplierFulfillmentItemId, SupplierFulfillmentOrder, SupplierFulfillmentOrderData,
    SupplierFulfillmentOrderId, SupplierFulfillmentOrderUpdate, SupplierOrderAction, SupplierOrderActionData,
    SupplierOrderActionLine, SupplierOrderActionLineData, SupplierOrderActionStatus, SupplierOrderActionType,
    SupplierOrderActionUpdate, SupplierOrderStatusHistory, SupplierOrderStatusHistoryData,
    SupplierRefundAllocation, SupplierRefundAllocationData, SupplierRefundFact, SupplierRefundFactData,
    VerifiedSupplierOrderResolution,
};
use entities::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::{WorkItemAllowedAction, WorkItemService};

pub(crate) mod dto;
mod gateway;

use self::dto::SortDir;

pub use self::dto::{
    AfterSalesActionLineRequest, PageView, PlaceFulfillmentOrderRequest, RecordRefundResultRequest,
    RecordSupplierRejectRequest, SubmitActionResultView, SubmitAfterSalesActionRequest,
    SupplierFulfillmentOrderDetailParams, SupplierFulfillmentOrderDetailView,
    SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderView, SupplierOrderActionBlockerView,
    SupplierOrderActionLineView, SupplierOrderActionView, SupplierOrderAddressView,
    SupplierOrderAllowedAction, SupplierOrderInvestigationAction, SupplierOrderInvestigationEvidenceView,
    SupplierOrderInvestigationOutcome, SupplierOrderInvestigationResultStatus,
    SupplierOrderInvestigationResultView, SupplierOrderInvestigationWorkItemView,
    SupplierOrderObjectInvestigationCommand, SupplierOrderResolution, SupplierOrderStatusHistoryView,
    SupplierOrderTaskCompletionCommand, SupplierOrderTaskCompletionResultView,
    SupplierOrderTaskInvestigationCommand, SupplierRefundAllocationView, SupplierRefundFactView,
};
pub use self::gateway::{
    DispatchOutcome, InvestigationOutcome, SimulatedSupplierGateway, SupplierGateway,
    UnavailableSupplierGateway,
};

const W26_BUSINESS_OBJECT_TYPE: &str = "SUPPLIER_FULFILLMENT_ORDER";
const W26_OWNER_ROLE: &str = "role-procurement";
const W26_OWNER_ORGANIZATION: &str = "company";
const INVESTIGATION_EVIDENCE_SCHEMA: &str = "W26_INVESTIGATION_V1";
const INVESTIGATION_INTENT_SCHEMA: &str = "W26_INVESTIGATION_INTENT_V1";
const INVESTIGATION_PREPARED_SCHEMA: &str = "W26_INVESTIGATION_PREPARED_V1";
const COMPLETION_EVIDENCE_SCHEMA: &str = "W26_TASK_COMPLETION_V1";
const INVESTIGATION_AUDIT_PREFIX: &str = "w26-investigation-";
const COMPLETION_AUDIT_PREFIX: &str = "w26-completion-";

#[derive(Debug, Clone)]
struct InvestigationCommandContext {
    order_id: String,
    expected_order_version: u64,
    action: SupplierOrderInvestigationAction,
    operation_id: String,
    target_action_id: String,
    task: Option<InvestigationTaskContext>,
}

#[derive(Debug, Clone)]
struct InvestigationTaskContext {
    work_item_id: String,
    expected_task_version: u64,
    expected_subject_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "result", rename_all = "SCREAMING_SNAKE_CASE")]
enum PreparedInvestigation {
    PersistedTerminal(SupplierOrderResolution),
    Queried(InvestigationOutcome),
    Replayed(DispatchOutcome),
}

#[derive(Debug, Clone)]
struct InvestigationFinding {
    outcome: SupplierOrderInvestigationOutcome,
    resolution: Option<SupplierOrderResolution>,
    summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InvestigationEvidenceRecord {
    schema: String,
    action: SupplierOrderInvestigationAction,
    target_supplier_action_id: String,
    outcome: SupplierOrderInvestigationOutcome,
    verified_resolution: Option<SupplierOrderResolution>,
    operation_id: String,
    summary: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompletionEvidenceRecord {
    schema: String,
    work_item_id: String,
    verified_evidence_id: String,
    resolution: SupplierOrderResolution,
}

#[derive(Debug, Clone)]
struct InvestigationReceipt {
    evidence_id: String,
    order_version: u64,
    task_version: Option<u64>,
}

#[derive(Debug, Clone)]
struct CompletionReceipt {
    terminal_action_id: String,
    order_version: u64,
    task_version: u64,
    resolution: SupplierOrderResolution,
}

impl From<VerifiedSupplierOrderResolution> for SupplierOrderResolution {
    /// 将实体层已验证业务终态映射为服务契约枚举。
    fn from(value: VerifiedSupplierOrderResolution) -> Self {
        match value {
            VerifiedSupplierOrderResolution::OrderAccepted => Self::OrderAccepted,
            VerifiedSupplierOrderResolution::OrderRejected => Self::OrderRejected,
            VerifiedSupplierOrderResolution::OrderCompleted => Self::OrderCompleted,
            VerifiedSupplierOrderResolution::Canceled => Self::Canceled,
            VerifiedSupplierOrderResolution::Refunded => Self::Refunded,
        }
    }
}

/// 履约订单列表筛选条件类型（经 `SupplierFulfillmentExt` 关联类型跨 crate 可达）。
type FulfillmentOrderFilter = <mongodb::Database as SupplierFulfillmentExt>::SupplierFulfillmentOrderFilter;

/// 返回零金额（成本余额累加起点）。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 返回零数量（售后申请行净余额累加起点）。
fn zero_quantity() -> Quantity {
    Quantity::from_str("0.000000").expect("零是合法数量")
}

/// 数量相减（小数位受类型约束，恒合法）。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回相减结果。
fn qty_sub(left: Quantity, right: Quantity) -> Quantity {
    Quantity::try_from(left.to_decimal() - right.to_decimal()).expect("数量小数位合法")
}

/// 金额相减（小数位受类型约束，恒合法）。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回相减结果。
fn amount_sub(left: Amount, right: Amount) -> Amount {
    Amount::try_from(left.to_decimal() - right.to_decimal()).expect("金额小数位合法")
}

/// 供应商履约服务。
///
/// 提供供应商子订单的下单、查询、取消/退款动作提交与外部结果登记编排。
pub struct SupplierFulfillmentService {
    db: Database,
    gateway: Arc<dyn SupplierGateway>,
}

impl SupplierFulfillmentService {
    /// 创建供应商履约服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `gateway` - 供应商动作派发网关（真实 Connector 接入点，只在事务外调用）
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, gateway: Arc<dyn SupplierGateway>) -> Self {
        Self { db, gateway }
    }

    /// 分页查询供应商履约订单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`supplier_id`/三条状态/`external_order_no` 等扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_list(
        &self,
        params: &SupplierFulfillmentOrderListParams,
    ) -> Result<PageView<SupplierFulfillmentOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = FulfillmentOrderFilter {
            supplier_id: query.supplier_id,
            fulfillment_status: query.fulfillment_status,
            external_order_no: query.external_order_no,
            mall_order_id: query.mall_order_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierFulfillmentOrderView {
                id: row.id,
                fulfillment_order_no: row.fulfillment_order_no,
                mall_order_id: row.mall_order_id.to_string(),
                supplier_id: row.supplier_id.to_string(),
                connection_id: row.connection_id.to_string(),
                split_no: row.split_no,
                fulfillment_status: row.fulfillment_status,
                cancel_status: row.cancel_status,
                refund_status: row.refund_status,
                external_order_no: row.external_order_no,
                submitted_at: row.submitted_at.map(|t| t.unix_secs()),
                accepted_at: row.accepted_at.map(|t| t.unix_secs()),
                completed_at: row.completed_at.map(|t| t.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询供应商履约订单详情（订单 + 明细 + 状态历史 + 动作 + 退款事实）。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_detail(
        &self,
        id: &str,
        params: &SupplierFulfillmentOrderDetailParams,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        let order = self.load_order(id).await?;
        let order_id = SupplierFulfillmentOrderId::new(id);
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(&order_id), &mut NoTransaction)
            .await?;
        let actions = self
            .db
            .supplier_order_actions()
            .list_by_order_newest(&order_id, &mut NoTransaction)
            .await?;
        let histories = self
            .db
            .supplier_order_status_histories()
            .list_by_order_chronological(&order_id, &mut NoTransaction)
            .await?;
        let refund_views = self.refund_views_for_order(&order_id).await?;

        let supplier_id = order.supplier_id.to_string();
        let supplier_name = self
            .db
            .supplier()
            .current_legal_names_by_account_ids(std::slice::from_ref(&order.supplier_id), &mut NoTransaction)
            .await?
            .remove(&supplier_id);
        let mall_order_no = self
            .db
            .mall_orders()
            .find_by_id(&order.mall_order_id, &mut NoTransaction)
            .await?
            .map(|mall_order| mall_order.external_order_no);
        let mut action_blockers = Vec::new();
        if supplier_name.is_none() {
            action_blockers.push(supplier_order_blocker(
                "VIEW_SUPPLIER_NAME",
                "SUPPLIER_NAME_MISSING",
                "供应商主体或当前名称修订缺失，禁止以供应商 ID 伪装名称",
            ));
        }
        if mall_order_no.is_none() {
            action_blockers.push(supplier_order_blocker(
                "VIEW_MALL_ORDER_NO",
                "MALL_ORDER_MISSING",
                "当前供应商履约单缺少可验证的商城订单事实",
            ));
        }
        action_blockers.push(supplier_order_blocker(
            "REVEAL_ADDRESS",
            "ADDRESS_REVEAL_NOT_REGISTERED",
            "当前 W26 尚未注册可审计的短时地址揭示入口",
        ));

        let target_action = actions
            .iter()
            .find(|action| action.action_type != SupplierOrderActionType::Query);
        let latest_investigation = target_action.and_then(|target| {
            actions.iter().find_map(|candidate| {
                let record = parse_investigation_evidence(candidate).ok()?;
                (record.target_supplier_action_id == target.base.id).then_some((candidate, record))
            })
        });
        let target_supplier_action_id = target_action.map(|action| action.base.id.clone());
        let last_investigation = latest_investigation
            .as_ref()
            .map(|(evidence, record)| investigation_evidence_view(&order, evidence, record));

        let work_item_id = params
            .work_item_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let formal = if let Some(work_item_id) = work_item_id {
            let view = WorkItemService::new(self.db.clone(), rbac)
                .work_item_detail(work_item_id, actor)
                .await?;
            if !matches!(
                view.work_item_type,
                WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
            ) || view.business_object_type != W26_BUSINESS_OBJECT_TYPE
                || view.business_object_id != order.base.id
                || view.subject_version != order.base.version.to_string()
                || false
            {
                return Err(Error::BusinessLogicError(
                    "正式任务与当前供应商履约订单不匹配".to_string(),
                ));
            }
            Some(view)
        } else {
            None
        };
        let mut allowed_actions = Vec::new();
        let can_process_formal = formal
            .as_ref()
            .is_some_and(|item| item.allowed_actions.contains(&WorkItemAllowedAction::Process));
        let has_active_task = self
            .db
            .work_items()
            .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, id, &mut NoTransaction)
            .await?
            .into_iter()
            .next()
            .is_some();
        let can_investigate = if formal.is_some() {
            if !can_process_formal {
                block_supplier_order_domain_actions(
                    &mut action_blockers,
                    "CURRENT_RESPONSIBILITY_REQUIRED",
                    "当前账号不是开放任务的当前责任人",
                );
                false
            } else {
                let raw = self
                    .db
                    .work_items()
                    .find_by_id(work_item_id.expect("formal work item id"), &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                if ensure_task_actor_eligible(&self.db, &raw, actor.id(), &mut NoTransaction)
                    .await
                    .is_err()
                {
                    block_supplier_order_domain_actions(
                        &mut action_blockers,
                        "ACTOR_INELIGIBLE",
                        "当前账号已不具备该供应商履约任务的角色或组织资格",
                    );
                    false
                } else {
                    true
                }
            }
        } else if has_active_task {
            block_supplier_order_domain_actions(
                &mut action_blockers,
                "FORMAL_WORK_ITEM_REQUIRED",
                "当前订单存在正式异常任务，必须从该待办携带明确任务身份进入",
            );
            false
        } else {
            true
        };

        if can_investigate {
            if let Some(target) = target_action {
                self.project_supplier_order_actions(
                    &order,
                    target,
                    latest_investigation.as_ref(),
                    formal.is_some(),
                    &mut allowed_actions,
                    &mut action_blockers,
                )
                .await?;
            } else {
                block_supplier_order_domain_actions(
                    &mut action_blockers,
                    "ORIGINAL_SUPPLIER_ACTION_MISSING",
                    "当前订单缺少可调查的原下单、取消或退款动作",
                );
            }
        }

        Ok(SupplierFulfillmentOrderDetailView {
            order: order.into(),
            items: items.into_iter().map(item_view).collect(),
            status_history: histories.into_iter().map(Into::into).collect(),
            actions: actions.into_iter().map(Into::into).collect(),
            refund_facts: refund_views,
            supplier_name,
            mall_order_no,
            address: SupplierOrderAddressView {
                masked: None,
                can_reveal: false,
                blocker_code: Some("ADDRESS_REVEAL_NOT_REGISTERED".to_string()),
                blocker_message: Some("当前 W26 尚未注册可审计的短时地址揭示入口".to_string()),
            },
            work_item: formal,
            target_supplier_action_id,
            last_investigation,
            allowed_actions,
            action_blockers,
        })
    }

    /// 以原供应商动作、连接能力和最新结构化证据投影 W26 动作。
    async fn project_supplier_order_actions(
        &self,
        order: &SupplierFulfillmentOrder,
        target: &SupplierOrderAction,
        latest_investigation: Option<&(&SupplierOrderAction, InvestigationEvidenceRecord)>,
        formal_entry: bool,
        allowed_actions: &mut Vec<SupplierOrderAllowedAction>,
        action_blockers: &mut Vec<SupplierOrderActionBlockerView>,
    ) -> Result<()> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?;
        let connection_active = connection.as_ref().is_some_and(SupplierApiConnection::is_active);
        let capabilities = if connection_active {
            self.db
                .supplier_api_capabilities()
                .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
                .await?
        } else {
            Vec::new()
        };

        if order.verified_resolution(target).is_some()
            || (connection_active
                && ensure_capability(&capabilities, SupplierApiCapabilityCode::Query).is_ok())
        {
            allowed_actions.push(SupplierOrderAllowedAction::QueryResult);
        } else {
            action_blockers.push(supplier_order_blocker(
                SupplierOrderAllowedAction::QueryResult.as_str(),
                if connection_active {
                    "QUERY_CAPABILITY_MISSING"
                } else {
                    "SUPPLIER_CONNECTION_UNAVAILABLE"
                },
                if connection_active {
                    "供应商连接未声明启用的结果查询能力"
                } else {
                    "供应商连接不存在或未启用，且当前业务事实尚不能证明终态"
                },
            ));
        }

        let replay_capability_ready = capability_for_action(target.action_type)
            .ok()
            .is_some_and(|needed| ensure_capability(&capabilities, needed).is_ok());
        if connection_active
            && replay_capability_ready
            && ensure_replay_safe(&self.db, order, target, &mut NoTransaction)
                .await
                .is_ok()
        {
            allowed_actions.push(SupplierOrderAllowedAction::Replay);
        } else {
            action_blockers.push(supplier_order_blocker(
                SupplierOrderAllowedAction::Replay.as_str(),
                "VERIFIED_NO_RESULT_REQUIRED",
                "只有最新调查证据明确证明原下单未形成结果，且原能力仍启用时才能重放",
            ));
        }

        let terminal_evidence = latest_investigation.and_then(|(evidence, record)| {
            let resolution = record.verified_resolution?;
            (verified_terminal_evidence(evidence, order, resolution).is_ok()
                && order.verified_resolution(target).map(Into::into) == Some(resolution))
            .then_some(resolution)
        });
        if formal_entry && terminal_evidence.is_some() {
            allowed_actions.push(SupplierOrderAllowedAction::ConfirmVerifiedTerminalResult);
        } else {
            action_blockers.push(supplier_order_blocker(
                SupplierOrderAllowedAction::ConfirmVerifiedTerminalResult.as_str(),
                if formal_entry {
                    "VERIFIED_TERMINAL_EVIDENCE_REQUIRED"
                } else {
                    "FORMAL_WORK_ITEM_REQUIRED"
                },
                if formal_entry {
                    "必须先取得与当前业务事实一致的最新已验证终态证据"
                } else {
                    "确认终态并完成任务只允许从明确的 W26 正式待办入口执行"
                },
            ));
        }
        Ok(())
    }

    /// 供应商下单（幂等键：`fulfillment_order_no`，§6.19）。
    ///
    /// 同事务创建子订单、全部明细、首个 `PLACE` 动作与 `inbox_message`；
    /// 事务外经网关派发供应商 API（P3 §7），结果经 `inbox_message` +
    /// `integration_error_task` 承接。重复提交（同一订单号）返回原订单当前视图，
    /// 不重复下单、不生成新单号。
    ///
    /// # 参数
    /// * `req` - 下单请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回下单后订单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 连接/商城订单/明细/供给修订不存在
    /// * `BusinessLogicError` - 连接未启用或缺少下单能力
    /// * `ConflictError` - 唯一键冲突（并发重复下单）
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn submit_place(
        &self,
        req: PlaceFulfillmentOrderRequest,
        actor: &AuditActor,
    ) -> Result<SupplierFulfillmentOrderView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .supplier_fulfillment_orders()
            .find_by_fulfillment_order_no(&req.fulfillment_order_no, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_no = %req.fulfillment_order_no, "下单幂等命中，返回原订单");
            return Ok(existing.into());
        }
        let (connection, offerings) = self.ensure_placeable(&req).await?;
        let (mut order, items, mut action) = self.build_place_facts(&req, &offerings)?;
        let mut message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.submit",
            "supplier_fulfillment_order",
            order.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let items_for_tx = items.clone();
        let action_for_tx = action.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment()
                        .create_fulfillment_with_items_and_place_action(
                            &order_for_tx,
                            &items_for_tx,
                            &action_for_tx,
                            session,
                        )
                        .await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        tracing::info!(account = %actor.id(), order_id = %order.base.id, "下单事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await?;
        Ok(order.into())
    }

    /// 提交供应商取消（幂等键：「订单号 + CANCEL + 售后申请 ID」，§6.19）。
    ///
    /// 同事务创建 `CANCEL` 动作头/行并把 `cancel_status` 推进到 `CANCEL_PENDING`；
    /// 事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 取消动作提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单/售后申请/申请行不存在
    /// * `BusinessLogicError` - 动作范围非法或连接缺少取消能力
    /// * `ConflictError` - 唯一键冲突（并发重复提交）
    pub async fn submit_cancel(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Cancel, actor)
            .await
    }

    /// 提交供应商退款（幂等键：「订单号 + REFUND + 售后申请 ID」，§6.19）。
    ///
    /// 同事务创建 `REFUND` 动作头/行并把 `refund_status` 推进到 `REFUND_PENDING`；
    /// 事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 退款动作提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单/售后申请/申请行不存在
    /// * `BusinessLogicError` - 动作范围非法或连接缺少退款能力
    /// * `ConflictError` - 唯一键冲突（并发重复提交）
    pub async fn submit_refund(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Refund, actor)
            .await
    }

    /// 从普通订单入口查询原结果或执行已证明安全的重放。
    ///
    /// 命令严格校验订单版本和原供应商动作；`REPLAY` 还必须存在最新的
    /// `VERIFIED_NO_RESULT` 查询证据，并始终沿原供应商动作幂等键派发。调查证据
    /// 与审计收据在同一事务提交，重复命令返回原结果。
    ///
    /// # 错误
    /// 订单/动作不存在、版本变化、查询能力不足、重放证据不足或幂等键复用不同
    /// 命令时失败关闭。
    pub async fn investigate_order(
        &self,
        command: SupplierOrderObjectInvestigationCommand,
        actor: &AuditActor,
    ) -> Result<SupplierOrderInvestigationResultView> {
        command.validate()?;
        let fingerprint = serialized_fingerprint(&command)?;
        let audit_id = investigation_audit_id(
            actor.id(),
            "supplier_fulfillment.investigate",
            command.order_id.as_ref(),
            &command.idempotency_key,
        );
        if let Some(result) = self
            .replay_investigation(&audit_id, &fingerprint, &command.operation_id, None)
            .await?
        {
            return Ok(result);
        }
        let context = InvestigationCommandContext {
            order_id: command.order_id.to_string(),
            expected_order_version: command.expected_lock_version,
            action: command.action,
            operation_id: command.operation_id,
            target_action_id: command.target_supplier_action_id.to_string(),
            task: None,
        };
        self.execute_investigation(context, audit_id, fingerprint, actor)
            .await
    }

    /// 从 W26 正式任务入口查询原结果或执行已证明安全的重放。
    ///
    /// 外部调用前和证据提交事务内均重验任务版本、主体版本、订单版本、当前个人
    /// 责任与角色/组织资格。证据、任务处理记录和审计收据同事务提交，任务始终
    /// 保持 `OPEN`。
    ///
    /// # 错误
    /// 正式任务未注册到当前订单、处理权变化、任一版本变化、调查证据不足或幂等
    /// 键复用不同命令时失败关闭；不得降级到普通对象动作。
    pub async fn investigate_order_task(
        &self,
        command: SupplierOrderTaskInvestigationCommand,
        actor: &AuditActor,
    ) -> Result<SupplierOrderInvestigationResultView> {
        command.validate()?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "任务版本")?;
        let fingerprint = serialized_fingerprint(&command)?;
        let audit_id = investigation_audit_id(
            actor.id(),
            "supplier_fulfillment.task_investigate",
            command.work_item_id.as_ref(),
            &command.idempotency_key,
        );
        let task_context = InvestigationTaskContext {
            work_item_id: command.work_item_id.to_string(),
            expected_task_version,
            expected_subject_version: command.expected_subject_version,
        };
        if let Some(result) = self
            .replay_investigation(
                &audit_id,
                &fingerprint,
                &command.action.operation_id,
                Some(&task_context),
            )
            .await?
        {
            return Ok(result);
        }
        let context = InvestigationCommandContext {
            order_id: command.action.order_id.to_string(),
            expected_order_version: command.action.expected_order_lock_version,
            action: command.action.action_type,
            operation_id: command.action.operation_id,
            target_action_id: command.action.target_supplier_action_id.to_string(),
            task: Some(task_context),
        };
        self.execute_investigation(context, audit_id, fingerprint, actor)
            .await
    }

    /// 以服务端可验证终态证据完成 W26 正式任务。
    ///
    /// 命令在一个事务中重验任务/主体/订单版本、当前责任、角色资格、证据身份和
    /// 当前业务终态，追加正式确认动作后完成原任务并写入稳定审计收据。证据仍
    /// 未知或任务误派时保持原任务开放。
    ///
    /// # 错误
    /// 任一任务、责任、版本、证据或终态不变量不成立，以及幂等键复用不同命令时
    /// 失败关闭。
    pub async fn complete_order_task(
        &self,
        command: SupplierOrderTaskCompletionCommand,
        actor: &AuditActor,
    ) -> Result<SupplierOrderTaskCompletionResultView> {
        command.validate()?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "任务版本")?;
        let fingerprint = serialized_fingerprint(&command)?;
        let audit_id = completion_audit_id(
            actor.id(),
            command.work_item_id.as_ref(),
            &command.idempotency_key,
        );
        if let Some(result) = self
            .replay_task_completion(&audit_id, &fingerprint, command.work_item_id.as_ref())
            .await?
        {
            return Ok(result);
        }

        let terminal_action_id = stable_evidence_id("w26c", &audit_id);
        let completion_idempotency_key = stable_internal_idempotency_key("w26c", &audit_id);
        let actor_id = actor.id().to_string();
        let actor_for_tx = actor.clone();
        let rbac_for_tx = crate::iam::shared_rbac_service(self.db.clone());
        let command_for_tx = command.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let terminal_action_id_for_tx = terminal_action_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut work_item = db
                        .work_items()
                        .find_by_id(command_for_tx.work_item_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                    validate_w26_task(
                        &work_item,
                        command_for_tx.decision.order_id.as_ref(),
                        expected_task_version,
                        &command_for_tx.expected_subject_version,
                        &actor_id,
                    )?;
                    ensure_task_actor_eligible(&db, &work_item, &actor_id, session).await?;
                    WorkItemService::new(db.clone(), rbac_for_tx.clone())
                        .ensure_domain_decision_access(&actor_for_tx, &work_item, session)
                        .await?;

                    let order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(command_for_tx.decision.order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order
                        .ensure_version(command_for_tx.decision.expected_order_lock_version)
                        .map_err(|_| {
                            Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string())
                        })?;
                    ensure_task_subject_matches_order(
                        &work_item,
                        &command_for_tx.expected_subject_version,
                        order.base.version,
                    )?;
                    let evidence = db
                        .supplier_order_actions()
                        .find_by_id(
                            command_for_tx
                                .decision
                                .verified_supplier_action_result_id
                                .as_ref(),
                            session,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结果证据不存在".to_string()))?;
                    let evidence_record =
                        verified_terminal_evidence(&evidence, &order, command_for_tx.decision.resolution)?;
                    let target_action = db
                        .supplier_order_actions()
                        .find_by_id(&evidence_record.target_supplier_action_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("结果证据引用的原供应商动作不存在".to_string()))?;
                    target_action
                        .ensure_original_for_order(&order.base.id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    if order.verified_resolution(&target_action).map(Into::into)
                        != Some(command_for_tx.decision.resolution)
                    {
                        return Err(Error::ConflictError(
                            "供应商结果已变化，请刷新证据后重试".to_string(),
                        ));
                    }

                    let completion_record = CompletionEvidenceRecord {
                        schema: COMPLETION_EVIDENCE_SCHEMA.to_string(),
                        work_item_id: work_item.base.id.clone(),
                        verified_evidence_id: evidence.base.id.clone(),
                        resolution: command_for_tx.decision.resolution,
                    };
                    let response_summary = serde_json::to_string(&completion_record)
                        .map_err(|error| Error::Internal(format!("任务完成证据序列化失败: {error}")))?;
                    let terminal_action = SupplierOrderAction::new(
                        SupplierOrderActionId::new(terminal_action_id_for_tx.clone()),
                        SupplierOrderActionData::query_result(
                            SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                            completion_idempotency_key,
                            format!("确认供应商结果证据 {}", evidence.base.id),
                            response_summary,
                        ),
                    )?;
                    let completed_at = Instant::now();
                    work_item.record_activity(&actor_id, completed_at)?;
                    work_item.complete_by_domain_command(&actor_id, completed_at)?;

                    db.supplier_order_actions()
                        .create(&terminal_action, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let receipt = CompletionReceipt {
                        terminal_action_id: terminal_action.base.id.clone(),
                        order_version: order.base.version,
                        task_version: work_item.base.version,
                        resolution: command_for_tx.decision.resolution,
                    };
                    let audit = actor_for_tx.resource_log_with_id(
                        audit_id_for_tx,
                        "supplier_fulfillment.task_complete",
                        W26_BUSINESS_OBJECT_TYPE,
                        order.base.id.clone(),
                        Some(completion_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CompletionReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(result) = self
                    .replay_task_completion(&audit_id, &fingerprint, command.work_item_id.as_ref())
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(completion_result(command.work_item_id.as_ref(), receipt))
    }

    /// 登记供应商拒单结果（回调幂等键 `(connection_id, external_event_id)`，§6.19）。
    ///
    /// 同事务推进履约主线到 `REJECTED`、追加状态历史并把原 `PLACE` 动作标记为
    /// 明确失败。重复回调（同一事件 ID）返回原状态历史，不重复推进。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 拒单结果请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新增的状态历史视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 当前履约状态不可拒单
    /// * `ConflictError` - 事件 ID 冲突（事务注入失败时全部不可见）
    pub async fn record_reject(
        &self,
        id: &str,
        req: RecordSupplierRejectRequest,
        actor: &AuditActor,
    ) -> Result<SupplierOrderStatusHistoryView> {
        req.validate()?;
        let mut order = self.load_order(id).await?;
        let connection_id = order.connection_id.clone();
        if let Some(existing) = self
            .db
            .supplier_order_status_histories()
            .find_by_connection_and_event(&connection_id, &req.external_event_id, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, "拒单回调幂等命中");
            return Ok(existing.into());
        }
        let previous = order.fulfillment_status;
        order.advance_fulfillment(FulfillmentStatus::Rejected)?;
        let history = SupplierOrderStatusHistory::new(
            SupplierOrderStatusHistoryId::new(next_id()),
            SupplierOrderStatusHistoryData::supplier_callback(
                SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                connection_id,
                previous,
                FulfillmentStatus::Rejected,
                req.supplier_status_version.clone(),
                Instant::from_unix_secs(req.occurred_at),
                Instant::now(),
                req.external_event_id.clone(),
            ),
        )?;
        let mut action = self.latest_place_action(id).await?;
        action.update(SupplierOrderActionUpdate {
            status: Some(SupplierOrderActionStatus::Failed),
            response_summary: Some("供应商明确拒单（回调登记）".to_string()),
            ..Default::default()
        })?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.reject",
            "supplier_fulfillment_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let history_for_tx = history.clone();
        let mut action_for_tx = action.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_order_status_histories()
                        .create(&history_for_tx, session)
                        .await?;
                    db.supplier_order_actions()
                        .update(&mut action_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(history.into())
    }

    /// 登记供应商退款成功结果（幂等键 `(connection_id, external_refund_no,
    /// external_refund_version)`，§6.19）。
    ///
    /// 同事务写入 `inbox_message`、退款事实头与全部分配行并推进退款进度
    /// （累计等于订单成本余额时为 `REFUNDED`，否则为 `PARTIAL`）；累计净退款
    /// 不得超过订单成本余额（§6.19）。重复登记返回原退款事实。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 退款成功结果请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回退款事实视图（含分配行）。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 退款金额超过订单成本净可退余额
    /// * `ConflictError` - 外部退款身份冲突（事务注入失败时全部不可见）
    pub async fn record_refund_result(
        &self,
        id: &str,
        req: RecordRefundResultRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundFactView> {
        req.validate()?;
        let mut order = self.load_order(id).await?;
        let connection_id = order.connection_id.clone();
        if let Some(existing) = self
            .db
            .supplier_refund_facts()
            .find_by_connection_and_refund(
                &connection_id,
                &req.external_refund_no,
                &req.external_refund_version,
                &mut NoTransaction,
            )
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, "退款结果幂等命中");
            let fact_id = SupplierRefundFactId::new(existing.base.id.as_str());
            let allocations = self
                .db
                .supplier_refund_allocations()
                .find_allocations_by_fact_ids(&[fact_id], &mut NoTransaction)
                .await?;
            return Ok(refund_fact_view(&existing, &allocations));
        }
        let financial = self
            .db
            .supplier_fulfillment()
            .refund_financial_snapshot(&SupplierFulfillmentOrderId::new(id), &mut NoTransaction)
            .await?;
        let order_total = financial.order_cost_gross;
        let refunded_total = financial.refunded_total;
        let total_after = refunded_total.checked_add(req.refund_amount);
        if total_after > order_total {
            return Err(Error::BusinessLogicError(
                "累计净退款金额不得超过订单成本余额".to_string(),
            ));
        }
        order.advance_refund(if total_after == order_total {
            RefundStatus::Refunded
        } else {
            RefundStatus::Partial
        })?;
        let message = build_refund_message(&order, &req, &connection_id, InboxMessageStatus::Received)?;
        let (fact, allocations) = self.build_refund_fact(&order, &req, &connection_id, &message)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.refund_result",
            "supplier_refund_fact",
            fact.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let fact_for_tx = fact.clone();
        let allocations_for_tx = allocations.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_fulfillment()
                        .create_refund_fact_with_allocations(&fact_for_tx, &allocations_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(refund_fact_view(&fact, &allocations))
    }

    /// 提交供应商取消/退款动作的公共编排。
    ///
    /// 完成幂等命中、售后申请存在性、连接能力、动作行净余额校验后，同事务写入
    /// 动作头/行与订单状态推进，事务外派发供应商 API。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 动作提交请求
    /// * `action_type` - 动作类型（`Cancel` 或 `Refund`）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// 同 [`Self::submit_cancel`]。
    async fn submit_after_sales_action(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        action_type: SupplierOrderActionType,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        req.validate()?;
        let mut order = self.load_order(id).await?;
        let idempotency_key = format!(
            "{}+{}+{}",
            order.fulfillment_order_no,
            action_type.as_str(),
            req.after_sales_request_id
        );
        if let Some(existing) = self
            .db
            .supplier_order_actions()
            .find_by_idempotency_key(&idempotency_key, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, action_type = %action_type.as_str(), "售后动作幂等命中");
            let lines = self
                .db
                .supplier_order_action_lines()
                .find_lines_by_action_ids(
                    &[SupplierOrderActionId::new(existing.base.id.as_str())],
                    &mut NoTransaction,
                )
                .await?;
            return Ok(SubmitActionResultView {
                action: existing.into(),
                lines: lines.into_iter().map(action_line_view).collect(),
                order: order.into(),
            });
        }
        self.db
            .mall_after_sales_requests()
            .find_by_id(&req.after_sales_request_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商城售后申请不存在".to_string()))?;
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
            .await?;
        let needed = match action_type {
            SupplierOrderActionType::Cancel => SupplierApiCapabilityCode::Cancel,
            SupplierOrderActionType::Refund => SupplierApiCapabilityCode::Refund,
            _ => return Err(Error::BusinessLogicError("不支持的售后动作类型".to_string())),
        };
        ensure_capability(&capabilities, needed)?;
        self.ensure_action_lines(&order, &req).await?;
        let mut action = self.build_after_sales_action(&order, &req, &idempotency_key, action_type)?;
        let lines = self.build_action_lines(&action, &req)?;
        match action_type {
            SupplierOrderActionType::Cancel => order.advance_cancel(CancelStatus::CancelPending)?,
            SupplierOrderActionType::Refund => order.advance_refund(RefundStatus::RefundPending)?,
            _ => {}
        }
        let mut message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.after_sales_action",
            "supplier_order_action",
            action.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let action_for_tx = action.clone();
        let lines_for_tx = lines.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_order_actions()
                        .create(&action_for_tx, session)
                        .await?;
                    for line in &lines_for_tx {
                        db.supplier_order_action_lines().create(line, session).await?;
                    }
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        tracing::info!(account = %actor.id(), action_id = %action.base.id, "售后动作事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await?;
        Ok(SubmitActionResultView {
            action: action.into(),
            lines: lines.into_iter().map(action_line_view).collect(),
            order: order.into(),
        })
    }
}

impl SupplierFulfillmentService {
    async fn execute_investigation(
        &self,
        context: InvestigationCommandContext,
        audit_id: String,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<SupplierOrderInvestigationResultView> {
        let evidence_id = stable_evidence_id("w26e", &audit_id);
        let evidence_idempotency_key = stable_internal_idempotency_key("w26e", &audit_id);
        let prepared = match self
            .ensure_investigation_intent(&context, &evidence_id, &evidence_idempotency_key, actor)
            .await?
        {
            Some(prepared) => prepared,
            None => {
                let prepared =
                    bounded_prepared_investigation(self.prepare_investigation(&context, actor).await?);
                self.persist_prepared_investigation(
                    &context,
                    &evidence_id,
                    &evidence_idempotency_key,
                    prepared,
                )
                .await?
            }
        };
        let context_for_tx = context.clone();
        let prepared_for_tx = prepared.clone();
        let actor_id = actor.id().to_string();
        let actor_for_tx = actor.clone();
        let rbac_for_tx = crate::iam::shared_rbac_service(self.db.clone());
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let evidence_id_for_tx = evidence_id.clone();
        let evidence_idempotency_key_for_tx = evidence_idempotency_key.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(&context_for_tx.order_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order
                        .ensure_version(context_for_tx.expected_order_version)
                        .map_err(|_| {
                            Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string())
                        })?;
                    let mut target_action = db
                        .supplier_order_actions()
                        .find_by_id(&context_for_tx.target_action_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
                    target_action
                        .ensure_original_for_order(&order.base.id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    let investigated_order_version = order.base.version;
                    if context_for_tx.task.is_none() {
                        ensure_no_active_w26_task(&db, &order.base.id, session).await?;
                    }
                    if context_for_tx.action == SupplierOrderInvestigationAction::Replay {
                        ensure_replay_safe(&db, &order, &target_action, session).await?;
                    }

                    let original_order = order.clone();
                    let original_target = target_action.clone();
                    let finding = apply_prepared_investigation(
                        &context_for_tx,
                        &prepared_for_tx,
                        &mut order,
                        &mut target_action,
                    )?;
                    if order != original_order {
                        db.supplier_fulfillment_orders()
                            .update(&mut order, session)
                            .await?;
                    }
                    if target_action != original_target {
                        db.supplier_order_actions()
                            .update(&mut target_action, session)
                            .await?;
                    }

                    let evidence_record = InvestigationEvidenceRecord {
                        schema: INVESTIGATION_EVIDENCE_SCHEMA.to_string(),
                        action: context_for_tx.action,
                        target_supplier_action_id: target_action.base.id.clone(),
                        outcome: finding.outcome,
                        verified_resolution: finding.resolution,
                        operation_id: context_for_tx.operation_id.clone(),
                        summary: bounded_summary(&finding.summary),
                    };
                    let response_summary = serde_json::to_string(&evidence_record)
                        .map_err(|error| Error::Internal(format!("调查证据序列化失败: {error}")))?;
                    let mut evidence = db
                        .supplier_order_actions()
                        .find_by_id(&evidence_id_for_tx, session)
                        .await?
                        .ok_or_else(|| Error::Internal("供应商调查意图记录不存在".to_string()))?;
                    validate_investigation_intent(
                        &evidence,
                        &context_for_tx,
                        &evidence_idempotency_key_for_tx,
                    )?;
                    let durable = parse_prepared_investigation(&evidence, &context_for_tx)?;
                    if durable != prepared_for_tx {
                        return Err(Error::ConflictError(
                            "供应商调查结果已由同一命令的另一执行确定".to_string(),
                        ));
                    }
                    evidence.update(SupplierOrderActionUpdate {
                        status: Some(evidence_action_status(finding.outcome)),
                        response_summary: Some(response_summary),
                        next_attempt_at: None,
                        ..Default::default()
                    })?;
                    db.supplier_order_actions().update(&mut evidence, session).await?;

                    let task_version = if let Some(task_context) = &context_for_tx.task {
                        let mut work_item = db
                            .work_items()
                            .find_by_id(&task_context.work_item_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                        validate_w26_task(
                            &work_item,
                            &order.base.id,
                            task_context.expected_task_version,
                            &task_context.expected_subject_version,
                            &actor_id,
                        )?;
                        ensure_task_subject_matches_order(
                            &work_item,
                            &task_context.expected_subject_version,
                            investigated_order_version,
                        )?;
                        ensure_task_actor_eligible(&db, &work_item, &actor_id, session).await?;
                        WorkItemService::new(db.clone(), rbac_for_tx.clone())
                            .ensure_domain_decision_access(&actor_for_tx, &work_item, session)
                            .await?;
                        work_item.subject_version = order.base.version.to_string();
                        work_item.record_activity(&actor_id, Instant::now())?;
                        db.work_items().update(&mut work_item, session).await?;
                        Some(work_item.base.version)
                    } else {
                        None
                    };
                    let receipt = InvestigationReceipt {
                        evidence_id: evidence.base.id.clone(),
                        order_version: order.base.version,
                        task_version,
                    };
                    let audit = actor_for_tx.resource_log_with_id(
                        audit_id_for_tx,
                        match context_for_tx.task {
                            Some(_) => "supplier_fulfillment.task_investigate",
                            None => "supplier_fulfillment.investigate",
                        },
                        W26_BUSINESS_OBJECT_TYPE,
                        order.base.id.clone(),
                        Some(investigation_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<
                        (
                            SupplierFulfillmentOrder,
                            SupplierOrderAction,
                            InvestigationEvidenceRecord,
                            Option<u64>,
                        ),
                        crate::errors::Error,
                    >((order, evidence, evidence_record, task_version))
                })
            })
            .await;

        let (order, evidence, record, task_version) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self
                    .replay_investigation(
                        &audit_id,
                        &fingerprint,
                        &context.operation_id,
                        context.task.as_ref(),
                    )
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(investigation_result(
            order,
            evidence,
            record,
            context.task.as_ref().map(|task| task.work_item_id.as_str()),
            task_version,
        ))
    }

    /// 在任何供应商查询或重放之前原子登记稳定调查意图。
    async fn ensure_investigation_intent(
        &self,
        context: &InvestigationCommandContext,
        evidence_id: &str,
        evidence_idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<PreparedInvestigation>> {
        let context = context.clone();
        let evidence_id = evidence_id.to_string();
        let evidence_idempotency_key = evidence_idempotency_key.to_string();
        let actor = actor.clone();
        let actor_id = actor.id().to_string();
        let rbac = crate::iam::shared_rbac_service(self.db.clone());
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(&context.order_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order
                        .ensure_version(context.expected_order_version)
                        .map_err(|_| {
                            Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string())
                        })?;
                    let target_action = db
                        .supplier_order_actions()
                        .find_by_id(&context.target_action_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
                    target_action
                        .ensure_original_for_order(&order.base.id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    if let Some(task_context) = &context.task {
                        let work_item = db
                            .work_items()
                            .find_by_id(&task_context.work_item_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                        validate_w26_task(
                            &work_item,
                            &order.base.id,
                            task_context.expected_task_version,
                            &task_context.expected_subject_version,
                            &actor_id,
                        )?;
                        ensure_task_subject_matches_order(
                            &work_item,
                            &task_context.expected_subject_version,
                            order.base.version,
                        )?;
                        ensure_task_actor_eligible(&db, &work_item, &actor_id, session).await?;
                        WorkItemService::new(db.clone(), rbac.clone())
                            .ensure_domain_decision_access(&actor, &work_item, session)
                            .await?;
                    } else {
                        ensure_no_active_w26_task(&db, &order.base.id, session).await?;
                    }
                    if context.action == SupplierOrderInvestigationAction::Replay {
                        ensure_replay_safe(&db, &order, &target_action, session).await?;
                    }

                    if let Some(existing) = db
                        .supplier_order_actions()
                        .find_by_id(&evidence_id, session)
                        .await?
                    {
                        validate_investigation_intent(&existing, &context, &evidence_idempotency_key)?;
                        return existing
                            .response_summary
                            .as_deref()
                            .map(|_| parse_prepared_investigation(&existing, &context))
                            .transpose();
                    }

                    let intent_summary = serde_json::to_string(&investigation_intent_record(&context))
                        .map_err(|error| Error::Internal(format!("调查意图序列化失败: {error}")))?;
                    let intent = SupplierOrderAction::new(
                        SupplierOrderActionId::new(evidence_id),
                        SupplierOrderActionData::query_intent(
                            SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                            evidence_idempotency_key,
                            intent_summary,
                        ),
                    )?;
                    db.supplier_order_actions().create(&intent, session).await?;
                    Ok(None)
                })
            })
            .await
    }

    /// 将事务外网关结果先持久化，再进入可能因任务或对象 CAS 失败的领域结算事务。
    async fn persist_prepared_investigation(
        &self,
        context: &InvestigationCommandContext,
        evidence_id: &str,
        evidence_idempotency_key: &str,
        prepared: PreparedInvestigation,
    ) -> Result<PreparedInvestigation> {
        let context = context.clone();
        let evidence_id = evidence_id.to_string();
        let evidence_idempotency_key = evidence_idempotency_key.to_string();
        let prepared_for_tx = prepared.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut evidence = db
                        .supplier_order_actions()
                        .find_by_id(&evidence_id, session)
                        .await?
                        .ok_or_else(|| Error::Internal("供应商调查意图记录不存在".to_string()))?;
                    validate_investigation_intent(&evidence, &context, &evidence_idempotency_key)?;
                    if evidence.response_summary.is_some() {
                        return parse_prepared_investigation(&evidence, &context);
                    }
                    let durable = DurablePreparedInvestigation {
                        schema: INVESTIGATION_PREPARED_SCHEMA.to_string(),
                        action: context.action,
                        target_supplier_action_id: context.target_action_id.clone(),
                        operation_id: context.operation_id.clone(),
                        prepared: prepared_for_tx.clone(),
                    };
                    let summary = serde_json::to_string(&durable)
                        .map_err(|error| Error::Internal(format!("调查结果序列化失败: {error}")))?;
                    evidence.update(SupplierOrderActionUpdate {
                        status: Some(SupplierOrderActionStatus::Pending),
                        response_summary: Some(summary),
                        next_attempt_at: None,
                        ..Default::default()
                    })?;
                    db.supplier_order_actions().update(&mut evidence, session).await?;
                    Ok(prepared_for_tx)
                })
            })
            .await
    }

    async fn prepare_investigation(
        &self,
        context: &InvestigationCommandContext,
        actor: &AuditActor,
    ) -> Result<PreparedInvestigation> {
        let order = self.load_order(&context.order_id).await?;
        order
            .ensure_version(context.expected_order_version)
            .map_err(|_| Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string()))?;
        let target_action = self
            .db
            .supplier_order_actions()
            .find_by_id(&context.target_action_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
        target_action
            .ensure_original_for_order(&order.base.id)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        if let Some(task_context) = &context.task {
            let work_item = self
                .db
                .work_items()
                .find_by_id(&task_context.work_item_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
            validate_w26_task(
                &work_item,
                &order.base.id,
                task_context.expected_task_version,
                &task_context.expected_subject_version,
                actor.id(),
            )?;
            ensure_task_subject_matches_order(
                &work_item,
                &task_context.expected_subject_version,
                order.base.version,
            )?;
            ensure_task_actor_eligible(&self.db, &work_item, actor.id(), &mut NoTransaction).await?;
        } else {
            ensure_no_active_w26_task(&self.db, &order.base.id, &mut NoTransaction).await?;
        }

        if context.action == SupplierOrderInvestigationAction::QueryResult {
            if let Some(resolution) = order.verified_resolution(&target_action).map(Into::into) {
                return Ok(PreparedInvestigation::PersistedTerminal(resolution));
            }
        } else {
            ensure_replay_safe(&self.db, &order, &target_action, &mut NoTransaction).await?;
        }

        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        if !connection.is_active() {
            return Err(Error::BusinessLogicError("供应商连接未启用".to_string()));
        }
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
            .await?;
        match context.action {
            SupplierOrderInvestigationAction::QueryResult => {
                ensure_capability(&capabilities, SupplierApiCapabilityCode::Query)?;
                Ok(PreparedInvestigation::Queried(
                    self.gateway
                        .investigate(&target_action, &order, &connection)
                        .await,
                ))
            }
            SupplierOrderInvestigationAction::Replay => {
                ensure_capability(&capabilities, capability_for_action(target_action.action_type)?)?;
                Ok(PreparedInvestigation::Replayed(
                    self.gateway.dispatch(&target_action, &order, &connection).await,
                ))
            }
        }
    }

    async fn replay_investigation(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_operation_id: &str,
        task: Option<&InvestigationTaskContext>,
    ) -> Result<Option<SupplierOrderInvestigationResultView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if !audit.success
            || audit.resource_type != W26_BUSINESS_OBJECT_TYPE
            || !matches!(
                audit.action.as_str(),
                "supplier_fulfillment.investigate" | "supplier_fulfillment.task_investigate"
            )
        {
            return Err(Error::Internal("W26 调查幂等收据身份非法".to_string()));
        }
        let receipt = parse_investigation_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("W26 调查幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        if task.is_some() != receipt.task_version.is_some() {
            return Err(Error::ConflictError("请求标识已用于不同的调查入口".to_string()));
        }
        let evidence = self
            .db
            .supplier_order_actions()
            .find_by_id(&receipt.evidence_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("W26 调查幂等证据不存在".to_string()))?;
        let record = parse_investigation_evidence(&evidence)?;
        if record.operation_id != expected_operation_id {
            return Err(Error::ConflictError("请求标识已用于不同的调查命令".to_string()));
        }
        let order = self
            .load_order(evidence.supplier_fulfillment_order_id.as_ref())
            .await?;
        if audit.resource_id.as_deref() != Some(order.base.id.as_str()) {
            return Err(Error::Internal("W26 调查幂等收据对象不一致".to_string()));
        }
        Ok(Some(investigation_result(
            order,
            evidence,
            record,
            task.map(|task| task.work_item_id.as_str()),
            receipt.task_version,
        )))
    }

    async fn replay_task_completion(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_work_item_id: &str,
    ) -> Result<Option<SupplierOrderTaskCompletionResultView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if !audit.success
            || audit.action != "supplier_fulfillment.task_complete"
            || audit.resource_type != W26_BUSINESS_OBJECT_TYPE
        {
            return Err(Error::Internal("W26 任务完成幂等收据身份非法".to_string()));
        }
        let receipt = parse_completion_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("W26 任务完成幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        let action = self
            .db
            .supplier_order_actions()
            .find_by_id(&receipt.terminal_action_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("W26 任务完成业务证据不存在".to_string()))?;
        let record = parse_completion_evidence(&action)?;
        if record.work_item_id != expected_work_item_id || record.resolution != receipt.resolution {
            return Err(Error::Internal(
                "W26 任务完成幂等收据与业务证据不一致".to_string(),
            ));
        }
        Ok(Some(completion_result(expected_work_item_id, receipt)))
    }

    /// 校验下单前置条件（跨域读取，P3 §2）并返回连接实体。
    ///
    /// D25 连接存在且启用并声明 `order` 能力；D29 商城订单与全部明细存在且归属一致；
    /// D24 全部供给修订存在。
    ///
    /// # 参数
    /// * `req` - 下单请求
    ///
    /// # 返回
    /// 返回已校验的供应商连接实体。
    ///
    /// # 错误
    /// * `NotFound` - 连接/商城订单/明细/供给修订不存在
    /// * `BusinessLogicError` - 连接未启用、缺少下单能力或明细归属不一致
    async fn ensure_placeable(
        &self,
        req: &PlaceFulfillmentOrderRequest,
    ) -> Result<(
        SupplierApiConnection,
        HashMap<String, entities::supplier_offering::SupplierOffering>,
    )> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&req.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        if !connection.is_active() {
            return Err(Error::BusinessLogicError("供应商连接未启用".to_string()));
        }
        if connection.supplier_id != req.supplier_id {
            return Err(Error::BusinessLogicError(
                "供应商连接不属于下单供应商".to_string(),
            ));
        }
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&req.connection_id, &mut NoTransaction)
            .await?;
        ensure_capability(&capabilities, SupplierApiCapabilityCode::Order)?;
        self.db
            .mall_orders()
            .find_by_id(&req.mall_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源商城订单不存在".to_string()))?;
        self.ensure_mall_items(req).await?;
        let mut revision_ids = req
            .items
            .iter()
            .map(|item| item.supplier_offering_revision_id.clone())
            .collect::<Vec<_>>();
        revision_ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        revision_ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        let by_revision = self
            .db
            .supplier_fulfillment()
            .load_offerings_by_revision_ids(&revision_ids, &mut NoTransaction)
            .await?;
        if by_revision.len() != revision_ids.len() {
            return Err(Error::NotFound("供应商供给修订或供给不存在".to_string()));
        }
        for offering in by_revision.values() {
            if !offering.belongs_to_ordering_source(&req.supplier_id, &req.connection_id) {
                return Err(Error::BusinessLogicError(
                    "供给不属于下单供应商或供应商连接不匹配".to_string(),
                ));
            }
        }
        Ok((connection, by_revision))
    }

    /// 校验商城订单明细全部存在且归属该商城订单（D29 跨域读取）。
    ///
    /// # 参数
    /// * `req` - 下单请求
    ///
    /// # 错误
    /// * `NotFound` - 商城订单明细不存在
    /// * `BusinessLogicError` - 明细不属于该商城订单
    async fn ensure_mall_items(&self, req: &PlaceFulfillmentOrderRequest) -> Result<()> {
        let item_ids = req
            .items
            .iter()
            .map(|item| item.mall_order_item_id.clone())
            .collect::<Vec<_>>();
        let items = self
            .db
            .mall_order_items()
            .list_by_ids(&item_ids, &mut NoTransaction)
            .await?;
        if items.len() != item_ids.len() {
            return Err(Error::NotFound("商城订单明细不存在".to_string()));
        }
        if items.iter().any(|item| item.mall_order_id != req.mall_order_id) {
            return Err(Error::BusinessLogicError(
                "商城订单明细不属于该商城订单".to_string(),
            ));
        }
        Ok(())
    }

    /// 构建下单事实（子订单 + 明细 + 首个 `PLACE` 动作）。
    ///
    /// 明细含税成本快照由单位成本 × 数量按分舍入派生（§4.2 铁律 1）；
    /// `PLACE` 动作幂等键为 ERP 供应商子订单号（§6.19）。
    ///
    /// # 参数
    /// * `req` - 下单请求
    ///
    /// # 返回
    /// 返回 `(子订单, 明细, 动作)` 三元组。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError` 或 `ValidationError`。
    fn build_place_facts(
        &self,
        req: &PlaceFulfillmentOrderRequest,
        offerings: &HashMap<String, entities::supplier_offering::SupplierOffering>,
    ) -> Result<(
        SupplierFulfillmentOrder,
        Vec<SupplierFulfillmentItem>,
        SupplierOrderAction,
    )> {
        let order_id = SupplierFulfillmentOrderId::new(next_id());
        let order = SupplierFulfillmentOrder::new(
            order_id.clone(),
            SupplierFulfillmentOrderData::submitting(
                req.fulfillment_order_no.clone(),
                req.mall_order_id.clone(),
                req.supplier_id.clone(),
                req.connection_id.clone(),
                req.split_no,
                Instant::now(),
                req.address_snapshot_encrypted.clone(),
                req.address_snapshot_fingerprint.clone(),
            ),
        )?;
        let items = self.build_place_items(&order_id, req, offerings)?;
        let action = SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData::place(order_id, req.fulfillment_order_no.clone(), items.len()),
        )?;
        Ok((order, items, action))
    }

    /// 构建下单明细（含税成本快照派生）。
    ///
    /// # 参数
    /// * `order_id` - 子订单 ID
    /// * `req` - 下单请求
    ///
    /// # 返回
    /// 返回明细集合。
    ///
    /// # 错误
    /// 数量/成本快照恒等校验失败时返回 `LogicError`。
    fn build_place_items(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        req: &PlaceFulfillmentOrderRequest,
        offerings: &HashMap<String, entities::supplier_offering::SupplierOffering>,
    ) -> Result<Vec<SupplierFulfillmentItem>> {
        req.items
            .iter()
            .map(|item| {
                let offering = offerings
                    .get(item.supplier_offering_revision_id.as_ref())
                    .ok_or_else(|| Error::NotFound("供应商供给不存在".to_string()))?;
                let data = SupplierFulfillmentItemData::from_unit_cost(
                    order_id.clone(),
                    item.mall_order_item_id.clone(),
                    item.supplier_offering_revision_id.clone(),
                    offering.supplier_sku_code.clone(),
                    offering.supplier_product_code.clone(),
                    item.quantity,
                    item.unit_cost_snapshot_gross,
                    item.input_tax_rate,
                )?;
                SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new(next_id()), data)
                    .map_err(Error::from)
            })
            .collect()
    }

    /// 构建取消/退款动作头。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    /// * `idempotency_key` - 「订单号 + 动作类型 + 售后申请 ID」
    /// * `action_type` - `Cancel` 或 `Refund`
    ///
    /// # 返回
    /// 返回动作实体。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError`。
    fn build_after_sales_action(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
        idempotency_key: &str,
        action_type: SupplierOrderActionType,
    ) -> Result<SupplierOrderAction> {
        SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData::after_sales(
                SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                action_type,
                req.after_sales_request_id.clone(),
                idempotency_key,
                req.reason_code.as_deref(),
            ),
        )
        .map_err(Into::into)
    }

    /// 构建取消/退款动作行（行号从 1 起，冻结实际提交范围）。
    ///
    /// # 参数
    /// * `action` - 动作头
    /// * `req` - 动作提交请求
    ///
    /// # 返回
    /// 返回动作行集合。
    ///
    /// # 错误
    /// 数量/金额非正的实体校验失败时返回 `LogicError`。
    fn build_action_lines(
        &self,
        action: &SupplierOrderAction,
        req: &SubmitAfterSalesActionRequest,
    ) -> Result<Vec<SupplierOrderActionLine>> {
        req.lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                SupplierOrderActionLine::new(
                    SupplierOrderActionLineId::new(next_id()),
                    SupplierOrderActionLineData::from_request_index(
                        SupplierOrderActionId::new(action.base.id.as_str()),
                        index,
                        line.after_sales_request_line_id.clone(),
                        line.supplier_fulfillment_item_id.clone(),
                        line.quantity,
                        line.amount,
                    ),
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()
            .map_err(crate::errors::Error::from)
    }

    /// 校验动作行范围（§6.19）：行明细必须属于该子订单；数量/金额不得超过
    /// 对应售后申请行尚未提交的净余额。范围事实（订单合法明细、申请行限额、
    /// 按申请行聚合的历史已提交数量/金额）由 Repository 一次取回
    /// （FUL-R03），Service 只保留跨聚合归属与剩余净额决定。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    ///
    /// # 错误
    /// * `NotFound` - 售后申请行不存在
    /// * `BusinessLogicError` - 明细归属或净余额超限
    async fn ensure_action_lines(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
    ) -> Result<()> {
        let scope = self
            .db
            .supplier_fulfillment()
            .after_sales_action_scope(
                &SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                &req.after_sales_request_id,
                &mut NoTransaction,
            )
            .await?;
        let item_ids: std::collections::HashSet<&str> = scope.item_ids.iter().map(|id| id.as_ref()).collect();
        for line in &req.lines {
            if !item_ids.contains(line.supplier_fulfillment_item_id.as_ref()) {
                return Err(Error::BusinessLogicError(
                    "动作行不属于该供应商子订单".to_string(),
                ));
            }
            let request_line = scope
                .request_line_limits
                .iter()
                .find(|request_line| request_line.id == line.after_sales_request_line_id)
                .ok_or_else(|| Error::NotFound("商城售后申请行不存在".to_string()))?;
            let (submitted_qty, submitted_amount) = scope
                .submitted_by_request_line
                .get(&line.after_sales_request_line_id)
                .map(|totals| (totals.quantity, totals.amount))
                .unwrap_or((zero_quantity(), zero_amount()));
            if line.quantity.to_decimal()
                > qty_sub(request_line.requested_quantity, submitted_qty).to_decimal()
                || line.amount.to_decimal()
                    > amount_sub(request_line.requested_amount, submitted_amount).to_decimal()
            {
                return Err(Error::BusinessLogicError(
                    "动作行数量或金额超过售后申请行尚未提交的净余额".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 构建退款事实头与全部分配行，并校验 APPLY 合计恒等（§6.19）。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 退款成功结果请求
    /// * `connection_id` - 供应商连接
    /// * `message` - 同事务创建的 `inbox_message` 信封
    ///
    /// # 返回
    /// 返回 `(事实头, 分配行)`。
    ///
    /// # 错误
    /// 金额恒等或实体校验失败时返回 `LogicError`。
    fn build_refund_fact(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &RecordRefundResultRequest,
        connection_id: &entities::ids::SupplierApiConnectionId,
        message: &InboxMessage,
    ) -> Result<(SupplierRefundFact, Vec<SupplierRefundAllocation>)> {
        let fact = SupplierRefundFact::new(
            SupplierRefundFactId::new(next_id()),
            SupplierRefundFactData {
                supplier_id: order.supplier_id.clone(),
                connection_id: connection_id.clone(),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                external_refund_no: req.external_refund_no.clone(),
                external_refund_version: req.external_refund_version.clone(),
                refund_amount: req.refund_amount,
                refunded_at: Instant::from_unix_secs(req.refunded_at),
                source_event_id: req.source_event_id.clone(),
                inbox_message_id: InboxMessageId::new(message.base.id.as_str()),
            },
        )?;
        let allocations = req
            .allocations
            .iter()
            .enumerate()
            .map(|(index, allocation)| {
                SupplierRefundAllocation::new(
                    SupplierRefundAllocationId::new(next_id()),
                    SupplierRefundAllocationData {
                        supplier_refund_fact_id: SupplierRefundFactId::new(fact.base.id.as_str()),
                        allocation_no: (index + 1) as u32,
                        supplier_fulfillment_item_id: allocation.supplier_fulfillment_item_id.clone(),
                        original_cost_entry_id: allocation.original_cost_entry_id.clone(),
                        original_cost_allocation_id: allocation.original_cost_allocation_id.clone(),
                        original_payable_entry_id: allocation.original_payable_entry_id.clone(),
                        original_payment_allocation_id: allocation.original_payment_allocation_id.clone(),
                        refund_quantity: allocation.refund_quantity,
                        gross_amount: allocation.gross_amount,
                        net_amount: allocation.net_amount,
                        tax_amount: allocation.tax_amount,
                        payable_reduction_amount: allocation.payable_reduction_amount,
                        cash_refund_amount: allocation.cash_refund_amount,
                        cash_supplier_refund_id: None,
                        allocation_action: entities::supplier_fulfillment::AllocationAction::Apply,
                        reverses_allocation_id: None,
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()?;
        fact.validate_allocations(&allocations)?;
        Ok((fact, allocations))
    }

    /// 事务外派发供应商动作并承接结果（P3 §7）。
    ///
    /// 网关调用不在任何事务闭包内；结果经 `inbox_message` +
    /// `integration_error_task` 承接后，在同一事务写回订单/动作/消息，并为人工
    /// 异常创建或复用 W26 正式任务。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新）
    /// * `action` - 供应商动作（就地更新）
    /// * `message` - `inbox_message` 信封（就地更新）
    /// * `connection` - 供应商连接
    /// * `actor` - 审计操作人
    ///
    /// # 错误
    /// 结果写回失败时返回 `ConflictError`/`RepositoryError`/`OutcomeUnknown`。
    async fn settle_dispatch(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        connection: &SupplierApiConnection,
        actor: &AuditActor,
    ) -> Result<()> {
        let outcome = self.gateway.dispatch(action, order, connection).await;
        tracing::info!(
            account = %actor.id(),
            order_id = %order.base.id,
            action_type = %action.action_type.as_str(),
            outcome = %outcome_label(&outcome),
            "供应商动作派发完成（事务外）"
        );
        let task = self.apply_dispatch_outcome(order, action, message, outcome)?;
        self.write_dispatch_result(order, action, message, task.as_ref(), actor)
            .await
    }

    /// 应用派发结果到订单/动作/消息（不落库），失败路径构造错误任务。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新）
    /// * `action` - 供应商动作（就地更新）
    /// * `message` - `inbox_message` 信封（就地更新）
    /// * `outcome` - 网关分类结果
    ///
    /// # 返回
    /// 失败路径返回待落库的错误任务，成功路径返回 `None`。
    ///
    /// # 错误
    /// 实体更新校验失败时返回 `LogicError`。
    fn apply_dispatch_outcome(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        outcome: DispatchOutcome,
    ) -> Result<Option<IntegrationErrorTask>> {
        match outcome {
            DispatchOutcome::Succeeded {
                external_request_id,
                external_order_no,
            } => {
                if action.action_type == SupplierOrderActionType::Place {
                    if let Some(order_no) = &external_order_no {
                        order.update(SupplierFulfillmentOrderUpdate {
                            external_order_no: Some(order_no.clone()),
                        })?;
                    }
                    order.advance_fulfillment(FulfillmentStatus::Accepted)?;
                }
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::Succeeded),
                    external_request_id: Some(external_request_id),
                    response_summary: Some("供应商已接单（模拟网关）".to_string()),
                    ..Default::default()
                })?;
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(Instant::now()),
                })?;
                Ok(None)
            }
            DispatchOutcome::Rejected { summary } => {
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::Failed),
                    response_summary: Some(summary),
                    ..Default::default()
                })?;
                if action.action_type == SupplierOrderActionType::Place {
                    order.advance_fulfillment(FulfillmentStatus::Rejected)?;
                }
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(Instant::now()),
                })?;
                Ok(None)
            }
            DispatchOutcome::ResultUnknown { summary } => {
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::ResultUnknown),
                    response_summary: Some(summary),
                    ..Default::default()
                })?;
                if action.action_type == SupplierOrderActionType::Place {
                    order.advance_fulfillment(FulfillmentStatus::ResultUnknown)?;
                }
                self.build_error_task(message, order, ErrorClass::ResultUnknown)
            }
            DispatchOutcome::Failed { error_class, summary } => {
                if error_class.can_auto_retry() {
                    action.record_attempt(Some(Instant::now()));
                } else {
                    action.update(SupplierOrderActionUpdate {
                        status: Some(SupplierOrderActionStatus::Failed),
                        response_summary: Some(summary),
                        ..Default::default()
                    })?;
                    if action.action_type == SupplierOrderActionType::Place {
                        order.advance_fulfillment(FulfillmentStatus::Exception)?;
                    }
                }
                self.build_error_task(message, order, error_class)
            }
        }
    }

    /// 构建失败路径错误任务并把消息置为失败（§6.21 错误分类）。
    ///
    /// # 参数
    /// * `message` - `inbox_message` 信封（置为失败）
    /// * `order` - 供应商子订单（业务对象引用）
    /// * `error_class` - 错误分类
    ///
    /// # 返回
    /// 返回待落库的错误任务。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError`。
    fn build_error_task(
        &self,
        message: &mut InboxMessage,
        order: &SupplierFulfillmentOrder,
        error_class: ErrorClass,
    ) -> Result<Option<IntegrationErrorTask>> {
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Failed),
            ..Default::default()
        })?;
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: Some(InboxMessageId::new(message.base.id.as_str())),
                business_object_id: Some(order.base.id.clone()),
                error_class,
                owner_role: None,
                owner_user_id: None,
            },
        )?;
        Ok(Some(task))
    }

    /// 在同一事务写回派发结果、错误事实、W26 正式任务与审计。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新并回读版本）
    /// * `action` - 供应商动作（就地更新并回读版本）
    /// * `message` - `inbox_message` 信封（就地更新并回读版本）
    /// * `task` - 失败路径错误任务；`None` 时消息按已处理写回
    /// * `actor` - 派发命令的审计操作人
    ///
    /// # 错误
    /// 乐观锁冲突/唯一键冲突透出 `ConflictError`，提交结果未知透出 `OutcomeUnknown`。
    async fn write_dispatch_result(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        task: Option<&IntegrationErrorTask>,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let mut action_for_tx = action.clone();
        let mut message_for_tx = message.clone();
        let task_for_tx = task.cloned();
        let work_item_id = WorkItemId::new(next_id());
        let task_audit_actor = actor.clone();
        let (order_out, action_out, message_out) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_order_actions()
                        .update(&mut action_for_tx, session)
                        .await?;
                    if let Some(task) = task_for_tx.as_ref() {
                        db.integration_ops()
                            .create_error_task_with_message_failure(task, &mut message_for_tx, session)
                            .await?;
                        let work_item_type = if task.error_class == ErrorClass::ResultUnknown {
                            WorkItemType::IntegrationResultUnknown
                        } else {
                            WorkItemType::BusinessException
                        };
                        let existing = db
                            .work_items()
                            .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, &order_for_tx.base.id, session)
                            .await?;
                        if let Some(mut work_item) = existing
                            .into_iter()
                            .find(|item| item.work_item_type == work_item_type)
                        {
                            let current_subject_version = order_for_tx.base.version.to_string();
                            if work_item.subject_version != current_subject_version {
                                work_item.subject_version = current_subject_version;
                                db.work_items().update(&mut work_item, session).await?;
                                let audit = task_audit_actor.clone().resource_log(
                                    "supplier_fulfillment.work_item.refresh_subject",
                                    "work_item",
                                    work_item.base.id.clone(),
                                )?;
                                db.audit_logs().create(&audit, session).await?;
                            }
                        } else {
                            let work_item = WorkItem::new(
                                work_item_id,
                                WorkItemData {
                                    work_item_type,
                                    business_object_type: W26_BUSINESS_OBJECT_TYPE.to_string(),
                                    business_object_id: order_for_tx.base.id.clone(),
                                    subject_version: order_for_tx.base.version.to_string(),
                                    owner_role: W26_OWNER_ROLE.to_string(),
                                    owner_organization_id: W26_OWNER_ORGANIZATION.to_string(),
                                    owner_user_id: task_audit_actor.id().to_string(),
                                    assignment_source: AssignmentSource::SystemRule,
                                    priority: WorkItemPriority::High,
                                    due_at: None,
                                    reason_code: Some(match work_item_type {
                                        WorkItemType::IntegrationResultUnknown => {
                                            "SUPPLIER_RESULT_UNKNOWN".to_string()
                                        }
                                        WorkItemType::BusinessException => {
                                            "SUPPLIER_BUSINESS_EXCEPTION".to_string()
                                        }
                                        _ => unreachable!(
                                            "W26 producer only creates registered exception tasks"
                                        ),
                                    }),
                                    impact_summary: Some(format!(
                                        "供应商订单 {} 需要核实原动作结果",
                                        order_for_tx.fulfillment_order_no
                                    )),
                                },
                            )?;
                            db.work_items().create(&work_item, session).await?;
                            let audit = task_audit_actor.clone().resource_log(
                                "supplier_fulfillment.work_item.create",
                                "work_item",
                                work_item.base.id.clone(),
                            )?;
                            db.audit_logs().create(&audit, session).await?;
                        }
                    } else {
                        db.inbox_messages().update(&mut message_for_tx, session).await?;
                    }
                    Ok::<(SupplierFulfillmentOrder, SupplierOrderAction, InboxMessage), crate::errors::Error>(
                        (order_for_tx, action_for_tx, message_for_tx),
                    )
                })
            })
            .await?;
        *order = order_out;
        *action = action_out;
        *message = message_out;
        Ok(())
    }

    /// 按子订单加载全部退款事实视图（含分配行）。
    ///
    /// 归组与排序由 `SupplierFulfillmentRepository::refund_fact_bundles_by_order`
    /// 承担；本方法只把归组快照映射为响应视图。
    ///
    /// # 参数
    /// * `order_id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回退款事实视图集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn refund_views_for_order(
        &self,
        order_id: &SupplierFulfillmentOrderId,
    ) -> Result<Vec<SupplierRefundFactView>> {
        let bundles = self
            .db
            .supplier_fulfillment()
            .refund_fact_bundles_by_order(order_id, &mut NoTransaction)
            .await?;
        Ok(bundles
            .into_iter()
            .map(|bundle| refund_fact_view(&bundle.fact, &bundle.allocations))
            .collect())
    }

    /// 按 ID 加载未删除供应商子订单。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回订单实体。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    async fn load_order(&self, id: &str) -> Result<SupplierFulfillmentOrder> {
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))
    }

    /// 加载该订单最近一次 `PLACE` 动作。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回 `PLACE` 动作实体。
    ///
    /// # 错误
    /// * `NotFound` - 不存在 `PLACE` 动作
    async fn latest_place_action(&self, id: &str) -> Result<SupplierOrderAction> {
        self.db
            .supplier_order_actions()
            .latest_by_order_and_type(
                &SupplierFulfillmentOrderId::new(id),
                SupplierOrderActionType::Place,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("该订单不存在下单动作".to_string()))
    }
}

fn ensure_version(actual: u64, expected: u64, object: &str) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{object}版本已变化，请刷新后重试")))
}

fn parse_positive_version(value: &str, field: &str) -> Result<u64> {
    let version = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{field}必须为正整数字符串")))?;
    if version == 0 {
        return Err(Error::ValidationError(format!("{field}必须为正整数字符串")));
    }
    Ok(version)
}

fn validate_w26_task(
    item: &WorkItem,
    order_id: &str,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor_id: &str,
) -> Result<()> {
    ensure_version(item.base.version, expected_task_version, "供应商履约任务")?;
    if item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("供应商履约任务已不是开放状态".to_string()));
    }
    if !matches!(
        item.work_item_type,
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
    ) || item.business_object_type != W26_BUSINESS_OBJECT_TYPE
        || item.business_object_id != order_id
        || false
    {
        return Err(Error::BusinessLogicError(
            "正式任务未注册到当前供应商履约订单".to_string(),
        ));
    }
    if item.subject_version != expected_subject_version {
        return Err(Error::ConflictError(
            "任务主体版本已变化，请刷新后重试".to_string(),
        ));
    }
    if !item.is_owned_by(actor_id) {
        return Err(Error::Forbidden(
            "当前用户不是该供应商履约任务的当前责任人".to_string(),
        ));
    }
    Ok(())
}

fn ensure_task_subject_matches_order(
    item: &WorkItem,
    expected_subject_version: &str,
    order_version: u64,
) -> Result<()> {
    let current_order_version = order_version.to_string();
    if item.subject_version == current_order_version && expected_subject_version == current_order_version {
        return Ok(());
    }
    Err(Error::ConflictError(
        "任务关联的订单版本已变化，请刷新后重试".to_string(),
    ))
}

async fn ensure_task_actor_eligible(
    db: &Database,
    item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, item, actor_id, executor);
    Ok(())
}

async fn ensure_no_active_w26_task(db: &Database, order_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let has_active_task = db
        .work_items()
        .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, order_id, executor)
        .await?
        .into_iter()
        .next()
        .is_some();
    if has_active_task {
        return Err(Error::ConflictError(
            "当前订单存在正式异常任务，必须使用任务调查命令".to_string(),
        ));
    }
    Ok(())
}

fn supplier_order_blocker(action: &str, code: &str, message: &str) -> SupplierOrderActionBlockerView {
    SupplierOrderActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        destination_workspace_id: None,
    }
}

fn block_supplier_order_domain_actions(
    blockers: &mut Vec<SupplierOrderActionBlockerView>,
    code: &str,
    message: &str,
) {
    for action in [
        SupplierOrderAllowedAction::QueryResult,
        SupplierOrderAllowedAction::Replay,
        SupplierOrderAllowedAction::ConfirmVerifiedTerminalResult,
    ] {
        blockers.push(supplier_order_blocker(action.as_str(), code, message));
    }
}

fn investigation_evidence_view(
    order: &SupplierFulfillmentOrder,
    evidence: &SupplierOrderAction,
    record: &InvestigationEvidenceRecord,
) -> SupplierOrderInvestigationEvidenceView {
    SupplierOrderInvestigationEvidenceView {
        evidence_id: evidence.base.id.clone(),
        target_supplier_action_id: record.target_supplier_action_id.clone(),
        outcome: record.outcome,
        recorded_at: i64::try_from(evidence.base.created_at).unwrap_or(i64::MAX),
        can_safe_retry: record.outcome == SupplierOrderInvestigationOutcome::VerifiedNoResult,
        external_order_no: order.external_order_no.clone(),
        summary: record.summary.clone(),
        verified_supplier_action_result_id: (record.outcome
            == SupplierOrderInvestigationOutcome::VerifiedTerminal)
            .then(|| evidence.base.id.clone()),
        verified_resolution: record.verified_resolution,
    }
}

fn capability_for_action(action_type: SupplierOrderActionType) -> Result<SupplierApiCapabilityCode> {
    match action_type {
        SupplierOrderActionType::Place => Ok(SupplierApiCapabilityCode::Order),
        SupplierOrderActionType::Cancel => Ok(SupplierApiCapabilityCode::Cancel),
        SupplierOrderActionType::Refund => Ok(SupplierApiCapabilityCode::Refund),
        SupplierOrderActionType::Query => Err(Error::BusinessLogicError(
            "查询记录不能作为再次提交的目标".to_string(),
        )),
    }
}

async fn ensure_replay_safe(
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
    Err(Error::BusinessLogicError(
        "尚无最新的明确无结果证据，禁止再次提交供应商下单".to_string(),
    ))
}

fn investigation_intent_record(context: &InvestigationCommandContext) -> InvestigationIntentRecord {
    InvestigationIntentRecord {
        schema: INVESTIGATION_INTENT_SCHEMA.to_string(),
        action: context.action,
        target_supplier_action_id: context.target_action_id.clone(),
        operation_id: context.operation_id.clone(),
    }
}

fn bounded_prepared_investigation(prepared: PreparedInvestigation) -> PreparedInvestigation {
    match prepared {
        PreparedInvestigation::Queried(InvestigationOutcome::VerifiedNoResult { summary }) => {
            PreparedInvestigation::Queried(InvestigationOutcome::VerifiedNoResult {
                summary: bounded_summary(&summary),
            })
        }
        PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown { summary }) => {
            PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown {
                summary: bounded_summary(&summary),
            })
        }
        PreparedInvestigation::Replayed(DispatchOutcome::Rejected { summary }) => {
            PreparedInvestigation::Replayed(DispatchOutcome::Rejected {
                summary: bounded_summary(&summary),
            })
        }
        PreparedInvestigation::Replayed(DispatchOutcome::ResultUnknown { summary }) => {
            PreparedInvestigation::Replayed(DispatchOutcome::ResultUnknown {
                summary: bounded_summary(&summary),
            })
        }
        PreparedInvestigation::Replayed(DispatchOutcome::Failed { error_class, summary }) => {
            PreparedInvestigation::Replayed(DispatchOutcome::Failed {
                error_class,
                summary: bounded_summary(&summary),
            })
        }
        other => other,
    }
}

fn validate_investigation_intent(
    evidence: &SupplierOrderAction,
    context: &InvestigationCommandContext,
    expected_idempotency_key: &str,
) -> Result<()> {
    if evidence.supplier_fulfillment_order_id.as_ref() != context.order_id
        || evidence.action_type != SupplierOrderActionType::Query
        || evidence.idempotency_key != expected_idempotency_key
    {
        return Err(Error::ConflictError("调查意图身份与当前命令不一致".to_string()));
    }
    let intent: InvestigationIntentRecord = serde_json::from_str(
        evidence
            .request_summary
            .as_deref()
            .ok_or_else(|| Error::Internal("调查意图摘要为空".to_string()))?,
    )
    .map_err(|_| Error::Internal("调查意图摘要格式无效".to_string()))?;
    if intent != investigation_intent_record(context) {
        return Err(Error::ConflictError("调查意图已用于不同的命令载荷".to_string()));
    }
    Ok(())
}

fn parse_prepared_investigation(
    evidence: &SupplierOrderAction,
    context: &InvestigationCommandContext,
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
        return Err(Error::ConflictError(
            "已持久化调查结果与当前命令不一致".to_string(),
        ));
    }
    Ok(durable.prepared)
}

fn apply_prepared_investigation(
    context: &InvestigationCommandContext,
    prepared: &PreparedInvestigation,
    order: &mut SupplierFulfillmentOrder,
    target_action: &mut SupplierOrderAction,
) -> Result<InvestigationFinding> {
    if context.action == SupplierOrderInvestigationAction::QueryResult {
        if let Some(resolution) = order.verified_resolution(target_action).map(Into::into) {
            return Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedTerminal,
                resolution: Some(resolution),
                summary: format!("已由当前供应商业务事实核实结果：{}", resolution.label()),
            });
        }
    }
    match prepared {
        PreparedInvestigation::PersistedTerminal(resolution) => {
            if order.verified_resolution(target_action).map(Into::into) != Some(*resolution) {
                return Err(Error::ConflictError(
                    "供应商业务结果已变化，请刷新后重试".to_string(),
                ));
            }
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedTerminal,
                resolution: Some(*resolution),
                summary: format!("已由当前供应商业务事实核实结果：{}", resolution.label()),
            })
        }
        PreparedInvestigation::Queried(InvestigationOutcome::VerifiedNoResult { summary }) => {
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::VerifiedNoResult,
                resolution: None,
                summary: summary.clone(),
            })
        }
        PreparedInvestigation::Queried(InvestigationOutcome::ResultUnknown { summary }) => {
            Ok(InvestigationFinding {
                outcome: SupplierOrderInvestigationOutcome::ResultUnknown,
                resolution: None,
                summary: summary.clone(),
            })
        }
        PreparedInvestigation::Replayed(outcome) => {
            apply_replay_outcome(order, target_action, outcome.clone())
        }
    }
}

fn apply_replay_outcome(
    order: &mut SupplierFulfillmentOrder,
    target_action: &mut SupplierOrderAction,
    outcome: DispatchOutcome,
) -> Result<InvestigationFinding> {
    match outcome {
        DispatchOutcome::Succeeded {
            external_request_id,
            external_order_no: Some(external_order_no),
        } => {
            order.update(SupplierFulfillmentOrderUpdate {
                external_order_no: Some(external_order_no),
            })?;
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
        }
        DispatchOutcome::Succeeded {
            external_order_no: None,
            ..
        } => {
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
        }
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
        }
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
        }
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
        }
    }
}

fn evidence_action_status(outcome: SupplierOrderInvestigationOutcome) -> SupplierOrderActionStatus {
    match outcome {
        SupplierOrderInvestigationOutcome::VerifiedTerminal
        | SupplierOrderInvestigationOutcome::VerifiedNoResult => SupplierOrderActionStatus::Succeeded,
        SupplierOrderInvestigationOutcome::ResultUnknown => SupplierOrderActionStatus::ResultUnknown,
    }
}

fn verified_terminal_evidence(
    evidence: &SupplierOrderAction,
    order: &SupplierFulfillmentOrder,
    expected_resolution: SupplierOrderResolution,
) -> Result<InvestigationEvidenceRecord> {
    if evidence.supplier_fulfillment_order_id.as_ref() != order.base.id
        || evidence.action_type != SupplierOrderActionType::Query
        || evidence.status != SupplierOrderActionStatus::Succeeded
    {
        return Err(Error::BusinessLogicError(
            "结果证据不属于当前供应商履约订单".to_string(),
        ));
    }
    let record = parse_investigation_evidence(evidence)?;
    if record.outcome != SupplierOrderInvestigationOutcome::VerifiedTerminal
        || record.verified_resolution != Some(expected_resolution)
    {
        return Err(Error::BusinessLogicError(
            "供应商证据尚未证明所选业务结果".to_string(),
        ));
    }
    Ok(record)
}

fn parse_investigation_evidence(action: &SupplierOrderAction) -> Result<InvestigationEvidenceRecord> {
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

fn parse_completion_evidence(action: &SupplierOrderAction) -> Result<CompletionEvidenceRecord> {
    if action.action_type != SupplierOrderActionType::Query
        || action.status != SupplierOrderActionStatus::Succeeded
    {
        return Err(Error::Internal("W26 任务完成证据身份非法".to_string()));
    }
    let record: CompletionEvidenceRecord = serde_json::from_str(
        action
            .response_summary
            .as_deref()
            .ok_or_else(|| Error::Internal("W26 任务完成证据为空".to_string()))?,
    )
    .map_err(|_| Error::Internal("W26 任务完成证据格式非法".to_string()))?;
    if record.schema != COMPLETION_EVIDENCE_SCHEMA {
        return Err(Error::Internal("W26 任务完成证据版本非法".to_string()));
    }
    Ok(record)
}

fn investigation_result(
    order: SupplierFulfillmentOrder,
    evidence: SupplierOrderAction,
    record: InvestigationEvidenceRecord,
    work_item_id: Option<&str>,
    task_version: Option<u64>,
) -> SupplierOrderInvestigationResultView {
    let result_status = match record.outcome {
        SupplierOrderInvestigationOutcome::VerifiedTerminal
        | SupplierOrderInvestigationOutcome::VerifiedNoResult => {
            SupplierOrderInvestigationResultStatus::Succeeded
        }
        SupplierOrderInvestigationOutcome::ResultUnknown => SupplierOrderInvestigationResultStatus::Unknown,
    };
    let allowed_actions = match record.outcome {
        SupplierOrderInvestigationOutcome::VerifiedTerminal => {
            vec!["CONFIRM_VERIFIED_TERMINAL_RESULT".to_string()]
        }
        SupplierOrderInvestigationOutcome::VerifiedNoResult => vec!["REPLAY".to_string()],
        SupplierOrderInvestigationOutcome::ResultUnknown => vec!["QUERY_RESULT".to_string()],
    };
    let work_item =
        work_item_id
            .zip(task_version)
            .map(|(id, task_version)| SupplierOrderInvestigationWorkItemView {
                id: id.to_string(),
                status: WorkItemStatus::Open,
                task_version,
            });
    SupplierOrderInvestigationResultView {
        result_status,
        message: record.summary.clone(),
        operation_id: record.operation_id.clone(),
        evidence: SupplierOrderInvestigationEvidenceView {
            evidence_id: evidence.base.id.clone(),
            target_supplier_action_id: record.target_supplier_action_id,
            outcome: record.outcome,
            recorded_at: i64::try_from(evidence.base.created_at).unwrap_or(i64::MAX),
            can_safe_retry: record.outcome == SupplierOrderInvestigationOutcome::VerifiedNoResult,
            external_order_no: order.external_order_no.clone(),
            summary: record.summary,
            verified_supplier_action_result_id: (record.outcome
                == SupplierOrderInvestigationOutcome::VerifiedTerminal)
                .then(|| evidence.base.id.clone()),
            verified_resolution: record.verified_resolution,
        },
        order: order.into(),
        work_item,
        allowed_actions,
        action_blockers: Vec::<SupplierOrderActionBlockerView>::new(),
    }
}

fn completion_result(
    work_item_id: &str,
    receipt: CompletionReceipt,
) -> SupplierOrderTaskCompletionResultView {
    SupplierOrderTaskCompletionResultView {
        operation_id: receipt.terminal_action_id,
        work_item_id: work_item_id.to_string(),
        work_item_status: WorkItemStatus::Completed,
        task_version: receipt.task_version,
        order_lock_version: receipt.order_version,
        resolution: receipt.resolution,
    }
}

fn serialized_fingerprint<T: Serialize>(command: &T) -> Result<String> {
    let bytes = serde_json::to_vec(command)
        .map_err(|error| Error::Internal(format!("命令指纹序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn investigation_audit_id(actor_id: &str, action: &str, object_id: &str, key: &str) -> String {
    format!(
        "{INVESTIGATION_AUDIT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{object_id}|{key}"))
    )
}

fn completion_audit_id(actor_id: &str, work_item_id: &str, key: &str) -> String {
    format!(
        "{COMPLETION_AUDIT_PREFIX}{}",
        stable_digest(&format!(
            "{actor_id}|supplier_fulfillment.task_complete|{work_item_id}|{key}"
        ))
    )
}

fn stable_evidence_id(prefix: &str, audit_id: &str) -> String {
    format!("{prefix}-{}", stable_digest(audit_id))
}

fn stable_internal_idempotency_key(prefix: &str, audit_id: &str) -> String {
    format!("{prefix}:{}", stable_digest(audit_id))
}

fn stable_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn investigation_receipt_message(fingerprint: &str, receipt: &InvestigationReceipt) -> String {
    format!(
        "fp={fingerprint};e={};o={};t={}",
        receipt.evidence_id,
        receipt.order_version,
        receipt
            .task_version
            .map(|version| version.to_string())
            .unwrap_or_else(|| "-".to_string())
    )
}

fn parse_investigation_receipt(message: &str, expected_fingerprint: &str) -> Result<InvestigationReceipt> {
    let fields = receipt_fields(message)?;
    if fields.get("fp").map(String::as_str) != Some(expected_fingerprint) {
        return Err(Error::ConflictError("请求标识已用于不同的调查命令".to_string()));
    }
    let evidence_id = fields
        .get("e")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| Error::Internal("W26 调查收据缺少证据ID".to_string()))?;
    let order_version = parse_receipt_version(fields.get("o"), "订单版本")?;
    let task_version = match fields.get("t").map(String::as_str) {
        Some("-") => None,
        Some(value) => Some(parse_positive_version(value, "收据任务版本")?),
        None => return Err(Error::Internal("W26 调查收据缺少任务版本".to_string())),
    };
    Ok(InvestigationReceipt {
        evidence_id,
        order_version,
        task_version,
    })
}

fn completion_receipt_message(fingerprint: &str, receipt: &CompletionReceipt) -> String {
    format!(
        "fp={fingerprint};a={};o={};t={};r={}",
        receipt.terminal_action_id,
        receipt.order_version,
        receipt.task_version,
        receipt.resolution.as_str()
    )
}

fn parse_completion_receipt(message: &str, expected_fingerprint: &str) -> Result<CompletionReceipt> {
    let fields = receipt_fields(message)?;
    if fields.get("fp").map(String::as_str) != Some(expected_fingerprint) {
        return Err(Error::ConflictError(
            "请求标识已用于不同的任务完成命令".to_string(),
        ));
    }
    let terminal_action_id = fields
        .get("a")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| Error::Internal("W26 完成收据缺少业务证据ID".to_string()))?;
    let order_version = parse_receipt_version(fields.get("o"), "订单版本")?;
    let task_version = parse_receipt_version(fields.get("t"), "任务版本")?;
    let resolution = parse_resolution(
        fields
            .get("r")
            .ok_or_else(|| Error::Internal("W26 完成收据缺少业务结果".to_string()))?,
    )?;
    Ok(CompletionReceipt {
        terminal_action_id,
        order_version,
        task_version,
        resolution,
    })
}

fn receipt_fields(message: &str) -> Result<HashMap<String, String>> {
    let mut fields = HashMap::new();
    for part in message.split(';') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| Error::Internal("W26 幂等收据格式非法".to_string()))?;
        if fields.insert(key.to_string(), value.to_string()).is_some() {
            return Err(Error::Internal("W26 幂等收据字段重复".to_string()));
        }
    }
    Ok(fields)
}

fn parse_receipt_version(value: Option<&String>, field: &str) -> Result<u64> {
    parse_positive_version(
        value
            .ok_or_else(|| Error::Internal(format!("W26 收据缺少{field}")))?
            .as_str(),
        field,
    )
    .map_err(|_| Error::Internal(format!("W26 收据{field}非法")))
}

fn parse_resolution(value: &str) -> Result<SupplierOrderResolution> {
    match value {
        "ORDER_ACCEPTED" => Ok(SupplierOrderResolution::OrderAccepted),
        "ORDER_REJECTED" => Ok(SupplierOrderResolution::OrderRejected),
        "ORDER_COMPLETED" => Ok(SupplierOrderResolution::OrderCompleted),
        "CANCELED" => Ok(SupplierOrderResolution::Canceled),
        "REFUNDED" => Ok(SupplierOrderResolution::Refunded),
        _ => Err(Error::Internal("W26 完成收据业务结果非法".to_string())),
    }
}

fn bounded_summary(value: &str) -> String {
    value.chars().take(512).collect()
}

/// 校验连接能力声明包含指定能力且为启用态（D25 跨域读取判定）。
///
/// # 参数
/// * `capabilities` - 连接能力集合
/// * `needed` - 所需能力代码
///
/// # 错误
/// 能力缺失或未启用时返回 `BusinessLogicError`。
fn ensure_capability(
    capabilities: &[SupplierApiCapability],
    needed: SupplierApiCapabilityCode,
) -> Result<()> {
    let supported = capabilities
        .iter()
        .any(|capability| capability.capability_code == needed && capability.is_active());
    if !supported {
        return Err(Error::BusinessLogicError(format!(
            "供应商连接缺少能力: {}",
            needed.as_str()
        )));
    }
    Ok(())
}

/// 构建动作 `inbox_message` 信封（P3 §7：事务内落消息，事务外派发）。
///
/// 来源身份取「supplier-api:{连接 ID}」，消息/事实键取动作幂等键，
/// 保证同一动作只产生一条正式记录。
///
/// # 参数
/// * `action` - 供应商动作
/// * `connection` - 供应商连接
/// * `status` - 初始消息状态（已接收）
///
/// # 返回
/// 返回消息实体。
///
/// # 错误
/// 实体构造校验失败时返回 `LogicError`。
fn build_action_message(
    action: &SupplierOrderAction,
    connection: &SupplierApiConnection,
    status: InboxMessageStatus,
) -> Result<InboxMessage> {
    Ok(InboxMessage::new(
        InboxMessageId::new(next_id()),
        InboxMessageData {
            source_system_id: supplier_source_system_id(connection),
            source_event_id: action.idempotency_key.clone(),
            message_type: MessageType::SupplierCallback,
            business_fact_key: action.idempotency_key.clone(),
            payload_schema_version: "1.0".to_string(),
            payload_reference: Some(format!("supplier-order-action:{}", action.base.id)),
            status,
            source_sent_at: None,
            received_at: Instant::now(),
            processed_at: None,
        },
    )?)
}

/// 构建退款结果 `inbox_message` 信封（来源事件取外部退款身份）。
///
/// # 参数
/// * `order` - 供应商子订单
/// * `req` - 退款成功结果请求
/// * `connection_id` - 供应商连接
/// * `status` - 初始消息状态（已接收）
///
/// # 返回
/// 返回消息实体。
///
/// # 错误
/// 实体构造校验失败时返回 `LogicError`。
fn build_refund_message(
    order: &SupplierFulfillmentOrder,
    req: &RecordRefundResultRequest,
    connection_id: &entities::ids::SupplierApiConnectionId,
    status: InboxMessageStatus,
) -> Result<InboxMessage> {
    let event_key = format!(
        "refund:{}:{}",
        req.external_refund_no, req.external_refund_version
    );
    Ok(InboxMessage::new(
        InboxMessageId::new(next_id()),
        InboxMessageData {
            source_system_id: SourceSystemId::new(format!("supplier-api:{connection_id}")),
            source_event_id: event_key.clone(),
            message_type: MessageType::SupplierCallback,
            business_fact_key: event_key,
            payload_schema_version: "1.0".to_string(),
            payload_reference: Some(format!("supplier-refund-order:{}", order.base.id)),
            status,
            source_sent_at: None,
            received_at: Instant::now(),
            processed_at: None,
        },
    )?)
}

/// 构造供应商来源系统 ID（连接派生，`supplier-api:{连接 ID}`）。
///
/// # 参数
/// * `connection` - 供应商连接
///
/// # 返回
/// 返回来源系统 ID。
fn supplier_source_system_id(connection: &SupplierApiConnection) -> entities::ids::SourceSystemId {
    entities::ids::SourceSystemId::new(format!("supplier-api:{}", connection.base.id))
}

/// 返回派发结果的简短标签（结构化日志用）。
///
/// # 参数
/// * `outcome` - 派发结果
///
/// # 返回
/// 返回标签字符串。
fn outcome_label(outcome: &DispatchOutcome) -> &'static str {
    match outcome {
        DispatchOutcome::Succeeded { .. } => "succeeded",
        DispatchOutcome::Rejected { .. } => "rejected",
        DispatchOutcome::ResultUnknown { .. } => "result_unknown",
        DispatchOutcome::Failed { .. } => "failed",
    }
}

/// 从履约明细实体构造响应视图。
///
/// # 参数
/// * `item` - 履约明细实体
///
/// # 返回
/// 返回响应视图。
fn item_view(item: SupplierFulfillmentItem) -> crate::supplier_fulfillment::dto::SupplierFulfillmentItemView {
    crate::supplier_fulfillment::dto::SupplierFulfillmentItemView {
        id: item.base.id,
        supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.to_string(),
        mall_order_item_id: item.mall_order_item_id.to_string(),
        supplier_offering_revision_id: item.supplier_offering_revision_id.to_string(),
        supplier_sku_code_snapshot: item.supplier_sku_code_snapshot,
        supplier_product_code_snapshot: item.supplier_product_code_snapshot,
        quantity: item.quantity,
        unit_cost_snapshot_gross: item.unit_cost_snapshot_gross,
        cost_snapshot_total_gross: item.cost_snapshot_total_gross,
        input_tax_rate: item.input_tax_rate,
    }
}

/// 从动作行实体构造响应视图。
///
/// # 参数
/// * `line` - 动作行实体
///
/// # 返回
/// 返回响应视图。
fn action_line_view(
    line: SupplierOrderActionLine,
) -> crate::supplier_fulfillment::dto::SupplierOrderActionLineView {
    crate::supplier_fulfillment::dto::SupplierOrderActionLineView {
        id: line.base.id,
        line_no: line.line_no,
        after_sales_request_line_id: line.after_sales_request_line_id.to_string(),
        supplier_fulfillment_item_id: line.supplier_fulfillment_item_id.to_string(),
        quantity: line.quantity,
        amount: line.amount,
    }
}

/// 从退款事实头与分配行构造响应视图。
///
/// # 参数
/// * `fact` - 退款事实头
/// * `allocations` - 分配行集合
///
/// # 返回
/// 返回响应视图。
fn refund_fact_view(
    fact: &SupplierRefundFact,
    allocations: &[SupplierRefundAllocation],
) -> crate::supplier_fulfillment::dto::SupplierRefundFactView {
    crate::supplier_fulfillment::dto::SupplierRefundFactView {
        id: fact.base.id.clone(),
        supplier_fulfillment_order_id: fact.supplier_fulfillment_order_id.to_string(),
        external_refund_no: fact.external_refund_no.clone(),
        external_refund_version: fact.external_refund_version.clone(),
        refund_amount: fact.refund_amount,
        refunded_at: fact.refunded_at.unix_secs(),
        source_event_id: fact.source_event_id.clone(),
        allocations: allocations.iter().map(refund_allocation_view).collect(),
        created_at: fact.base.created_at,
    }
}

/// 从退款分配行实体构造响应视图。
///
/// # 参数
/// * `allocation` - 退款分配行实体
///
/// # 返回
/// 返回响应视图。
fn refund_allocation_view(
    allocation: &SupplierRefundAllocation,
) -> crate::supplier_fulfillment::dto::SupplierRefundAllocationView {
    crate::supplier_fulfillment::dto::SupplierRefundAllocationView {
        id: allocation.base.id.clone(),
        allocation_no: allocation.allocation_no,
        supplier_fulfillment_item_id: allocation.supplier_fulfillment_item_id.to_string(),
        refund_quantity: allocation.refund_quantity,
        gross_amount: allocation.gross_amount,
        net_amount: allocation.net_amount,
        tax_amount: allocation.tax_amount,
        payable_reduction_amount: allocation.payable_reduction_amount,
        cash_refund_amount: allocation.cash_refund_amount,
        allocation_action: allocation.allocation_action,
    }
}

#[cfg(test)]
mod investigation_tests {
    use super::{
        investigation_intent_record, parse_prepared_investigation, validate_investigation_intent,
        DurablePreparedInvestigation, InvestigationCommandContext, PreparedInvestigation,
        INVESTIGATION_PREPARED_SCHEMA,
    };
    use crate::supplier_fulfillment::{InvestigationOutcome, SupplierOrderInvestigationAction};
    use entities::{
        ids::{SupplierFulfillmentOrderId, SupplierOrderActionId},
        supplier_fulfillment::{
            SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionStatus, SupplierOrderActionType,
        },
    };

    fn context() -> InvestigationCommandContext {
        InvestigationCommandContext {
            order_id: "order-1".to_string(),
            expected_order_version: 3,
            action: SupplierOrderInvestigationAction::QueryResult,
            operation_id: "operation-1".to_string(),
            target_action_id: "target-action-1".to_string(),
            task: None,
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
                after_sales_request_id: None,
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
        assert_eq!(
            parse_prepared_investigation(&evidence, &context).unwrap(),
            prepared
        );

        let mut changed = context;
        changed.operation_id = "operation-2".to_string();
        assert!(validate_investigation_intent(&evidence, &changed, "stable-key").is_err());
    }
}
