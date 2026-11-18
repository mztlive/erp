//! 原采购权限内的接口执行与已应用履约数量，不包含供应商凭据、地址或资金账号。

use erp_core::money::Quantity;
use erp_procurement::entity::purchase_order::{
    FulfillmentResponsibility, PurchaseOrder, PurchaseOrderRevisionLine,
};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage, ConfirmationActionState};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::ports::connector::order::ConfirmationState;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseApiExecutionView {
    pub order_id: String,
    pub order_no: String,
    pub detail_available: bool,
    pub stage: ApiOrderStage,
    pub blocker: Option<String>,
    pub current_order_count: usize,
    pub confirmed_order_count: usize,
}

/// 数量来自已过账收货或已发货/签收的ERP发货单；不适用或无法安全归集时为空。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseFulfillmentQuantityView {
    pub line_id: String,
    pub product_name: Option<String>,
    pub unit: Option<String>,
    pub ordered_quantity: Option<Quantity>,
    pub inbound_quantity: Option<Quantity>,
    pub shipped_quantity: Option<Quantity>,
    pub remaining_quantity: Option<Quantity>,
    pub blocker: Option<String>,
}

impl PurchaseApiExecutionView {
    /// 从当前采购的关联事实生成摘要，尚未初始化的排队任务不要求履约单存在。
    /// # 参数
    /// `execution`、`connection`、`root` 为服务端反查的执行、连接和可选履约单。
    /// `purchase`、`current` 为已经授权的采购单及当前修订明细。
    /// # 返回
    /// 返回归属已核验的执行摘要；排队任务不提供尚未生成的详情入口。
    /// # 错误
    /// 来源不属当前采购、连接或修订时拒绝；非原始排队任务缺履约单时拒绝。
    pub fn from_purchase_execution(
        execution: &ApiOrderExecution,
        purchase: &PurchaseOrder,
        current: &[PurchaseOrderRevisionLine],
        connection: &SupplierApiConnection,
        root: Option<&SupplierFulfillmentOrder>,
    ) -> Result<Self> {
        if execution.purchase_order_id != purchase.base.id
            || execution.supplier_id != purchase.supplier_id.as_ref()
            || Some(execution.purchase_order_revision_id.as_str())
                != purchase.stable.current_revision_id.as_deref()
            || connection.base.id != execution.connection_id
            || connection.supplier_id != purchase.supplier_id
            || execution.source_lines.is_empty()
            || execution.source_lines.iter().any(|source| {
                !current.iter().any(|line| {
                    line.base.id == source.purchase_order_revision_line_id
                        && line.purchase_order_revision_id.as_ref() == execution.purchase_order_revision_id
                })
            })
            || root.is_some_and(|root| {
                root.base.id != execution.base.id
                    || root.connection_id.as_ref() != execution.connection_id
                    || root.supplier_id != purchase.supplier_id
            })
        {
            return Err(Error::Forbidden(
                "自动下单记录不属于这张采购单的当前版本或供应商，请从采购单详情重新打开".into(),
            ));
        }
        match root {
            Some(root) => Ok(Self::from_execution(execution, &root.fulfillment_order_no)),
            None => Self::from_queued_execution(execution),
        }
    }

    /// 从已核验采购归属的执行及履约单生成安全摘要。
    /// # 参数
    /// `execution` 为当前采购版本执行；`order_no` 为反查得到的原履约业务单号。
    /// # 返回
    /// 仅阶段、子单确认数量及不包含原始金额或个人资料的异常提示。
    /// # 错误
    /// 不返回错误；调用前由 `from_purchase_execution` 统一核验归属。
    fn from_execution(execution: &ApiOrderExecution, order_no: &str) -> Self {
        let leaves = execution.order_tree.as_ref().map(|tree| tree.leaves()).unwrap_or_default();
        let confirmed = leaves
            .iter()
            .filter(|leaf| {
                leaf.confirmation == Some(ConfirmationState::Confirmed)
                    || execution
                        .confirmations
                        .get(&leaf.external_order_no)
                        .is_some_and(|action| action.state == ConfirmationActionState::Confirmed)
            })
            .count();

        let blocker = match execution.stage {
            ApiOrderStage::CreationUnknown
            | ApiOrderStage::AddressUnknown
            | ApiOrderStage::PaymentUnknown
            | ApiOrderStage::ConfirmationUnknown => Some("原请求结果待核实，请查询原结果".into()),
            ApiOrderStage::AwaitingSetup => Some("请补齐本采购的履约资料".into()),
            _ => execution.blocker.as_ref().map(|_| "执行条件待处理，请由有权限的采购或财务人员核对".into()),
        };

        Self {
            order_id: execution.base.id.clone(),
            order_no: order_no.into(),
            detail_available: true,
            stage: execution.stage,
            blocker,
            current_order_count: leaves.len(),
            confirmed_order_count: confirmed,
        }
    }

    fn from_queued_execution(execution: &ApiOrderExecution) -> Result<Self> {
        if execution.stage != ApiOrderStage::Queued
            || execution.request_encrypted.is_some()
            || execution.address_fingerprint.is_some()
            || execution.setup_idempotency_key.is_some()
            || execution.setup_payload_hash.is_some()
            || !execution.action_hashes.is_empty()
            || execution.original_result_query_allowed()
            || execution.cancellation_action_id.is_some()
            || execution.cancellation_payload_hash.is_some()
            || execution.superseded_by_purchase_revision_id.is_some()
        {
            return Err(Error::NotFound(
                "找不到本次自动下单生成的履约单，请联系采购经办人核对原下单结果，勿重复下单".into(),
            ));
        }
        let mut view = Self::from_execution(execution, "");
        view.detail_available = false;
        Ok(view)
    }
}

impl PurchaseFulfillmentQuantityView {
    /// 由冻结采购行及已经应用的ERP数量生成逐行展示，不读取供应商状态。
    /// # 参数
    /// `line` 为当前版本行；`inbound`、`shipped` 仅来自适用的完整ERP事实查询。
    /// `ambiguous` 表示多个当前行共用稳定归集身份，不能安全重复分配历史数量。
    /// # 返回
    /// 适用轨道和剩余数量；缺少依据、归集冲突或超量时保留空值及核对提示。
    /// # 错误
    /// 负数量、十进制溢出或不适用轨道混入数量时返回错误。
    pub fn from_applied_facts(
        line: &PurchaseOrderRevisionLine,
        responsibility: FulfillmentResponsibility,
        inbound: Option<Quantity>,
        shipped: Option<Quantity>,
        ambiguous: bool,
    ) -> Result<Self> {
        if [line.quantity, inbound, shipped]
            .into_iter()
            .flatten()
            .any(|value| value.to_decimal() < Decimal::ZERO)
            || (inbound.is_some() && responsibility != FulfillmentResponsibility::Warehouse)
            || (shipped.is_some() && responsibility != FulfillmentResponsibility::SupplierDirect)
        {
            return Err(Error::BusinessLogicError(
                "履约数量或交付方式与采购单不一致，请核对收货和发货记录".into(),
            ));
        }

        let completed = match responsibility {
            FulfillmentResponsibility::Warehouse => inbound,
            FulfillmentResponsibility::SupplierDirect => shipped,
            _ => None,
        };
        let mut blocker = if ambiguous {
            Some("历史收货或发货记录无法确认归属，暂不计入本行；请核对原履约记录".into())
        } else if line.quantity.is_none() || completed.is_none() {
            Some("缺少采购数量、收货或发货记录，暂无法计算剩余数量，请核对原记录".into())
        } else {
            None
        };

        let remaining_quantity = match (line.quantity, completed) {
            (Some(ordered), Some(completed)) if !ambiguous && completed <= ordered => {
                let remaining = ordered
                    .to_decimal()
                    .checked_sub(completed.to_decimal())
                    .ok_or_else(|| Error::BusinessLogicError("采购剩余数量超出范围".into()))?;
                Some(Quantity::try_from(remaining).map_err(Error::Logic)?)
            },
            (Some(ordered), Some(completed)) if completed > ordered => {
                blocker = Some("已收货或发货的数量超过当前采购数量，请核对采购变更及退回记录".into());
                None
            },
            _ => None,
        };
        Ok(Self {
            line_id: line.base.id.clone(),
            product_name: line.product_name_snapshot.clone(),
            unit: line.base_unit_code.clone(),
            ordered_quantity: line.quantity,
            inbound_quantity: inbound,
            shipped_quantity: shipped,
            remaining_quantity,
            blocker,
        })
    }
}
