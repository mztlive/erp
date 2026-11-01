use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::{Amount, round_to_cent};
use serde::{Deserialize, Serialize};

use super::identity;
use crate::dto::supplier_fulfillment::SubmitAfterSalesActionRequest;
use crate::entity::supplier_fulfillment::{SupplierFulfillmentItem, SupplierFulfillmentOrder};
use crate::ports::connector::common::{ActionKey, Snapshot, SourceRevision, SourceStamp, SupplierSku};
use crate::ports::connector::order::{OrderReference, RefundReceipt, RequestRefund, ReturnLine};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundExecutionState {
    Ready,
    Unknown,
    Accepted,
    Unsupported,
    Rejected,
    Received,
    Applied,
}

/// 人工核实来源必须明确标识，不能伪装成供应商 API 查询证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualRefundEvidence {
    pub reference: String,
    pub recorded_by: String,
    pub external_version: String,
}

/// 一笔售后动作固定一次发送身份。Received 只表示来源已证明退款，尚未分配财务事实。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct RefundExecution {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub supplier_id: String,
    pub request: RequestRefund,
    pub line_amounts: Vec<Amount>,
    pub state: RefundExecutionState,
    pub receipt: Option<Snapshot<RefundReceipt>>,
    pub external_request_id: Option<String>,
    pub blocker: Option<String>,
    pub next_run_at: Option<Instant>,
    pub application_hash: Option<String>,
    pub fact_id: Option<String>,
    pub manual_evidence: Option<ManualRefundEvidence>,
}

impl RefundExecution {
    /// 从正式订单明细构造稳定退款动作，不接受前端提供供应商 SKU 或账号。
    /// # 参数
    /// `connection`、`supplier`、`order` 为正式身份；`input` 为范围；`items` 为原明细。
    /// # 返回
    /// 已校验金额与数量的未发送动作，同载荷生成同一身份。
    /// # 错误
    /// 重复、越界、超额、空原因或序列化失败时拒绝。
    pub fn prepare(
        connection: &str,
        supplier: &str,
        order: OrderReference,
        input: &SubmitAfterSalesActionRequest,
        items: &[SupplierFulfillmentItem],
    ) -> Result<Self> {
        let reason = input.reason_code.as_deref().unwrap_or_default().trim();
        if reason.is_empty() || reason.len() > 128 || input.lines.is_empty() || input.lines.len() > 1000 {
            return Err(Error::ValidationError("退款原因和1至1000条原订单明细为必填".into()));
        }
        let (lines, amounts) = prepare_lines(input, items)?;
        let amount = sum_amounts(&amounts)?;
        let payload = serde_json::to_string(&(&order, &lines, &amounts, reason))
            .map_err(|_| Error::ValidationError("退款载荷无法冻结".into()))?;
        let hash = identity(&[connection, &payload]);
        let key = input.request_id.as_deref().map(str::trim).unwrap_or(&hash);
        if key.is_empty() || key.len() > 128 {
            return Err(Error::ValidationError("退款申请身份无效".into()));
        }
        let id = format!("api-refund-{}", identity(&[connection, &order.merchant_order_no, key]));
        Ok(Self {
            base: BaseModel::new(id.clone()),
            connection_id: connection.into(),
            supplier_id: supplier.into(),
            request: RequestRefund {
                action: ActionKey { id, payload_hash: hash },
                order,
                lines,
                amount,
                reason_code: reason.into(),
            },
            line_amounts: amounts,
            state: RefundExecutionState::Ready,
            receipt: None,
            external_request_id: None,
            blocker: None,
            next_run_at: Some(Instant::now()),
            application_hash: None,
            fact_id: None,
            manual_evidence: None,
        })
    }

    /// 核对同一订单的累计申请，未决动作占用范围，明确未发送或拒绝不占用。
    /// # 参数
    /// `previous` 为同订单全部原动作；`items` 为原订单明细。
    /// # 返回
    /// 数量及金额均未超过原采购时成功。
    /// # 错误
    /// 历史范围损坏、累计超额或定点溢出时拒绝。
    pub fn ensure_remaining(&self, previous: &[Self], items: &[SupplierFulfillmentItem]) -> Result<()> {
        for (index, line) in self.request.lines.iter().enumerate() {
            let item = items
                .iter()
                .find(|i| i.base.id == line.line_id)
                .ok_or_else(|| Error::ValidationError("原退款明细不存在".into()))?;
            let mut quantity = line.quantity.to_decimal();
            let mut amounts = vec![self.line_amounts[index]];
            for old in previous.iter().filter(|old| {
                old.base.id != self.base.id
                    && !matches!(
                        old.state,
                        RefundExecutionState::Unsupported | RefundExecutionState::Rejected
                    )
            }) {
                for (i, old_line) in
                    old.request.lines.iter().enumerate().filter(|(_, l)| l.line_id == line.line_id)
                {
                    quantity = quantity
                        .checked_add(old_line.quantity.to_decimal())
                        .ok_or_else(|| Error::ValidationError("退款数量溢出".into()))?;
                    amounts.push(
                        *old.line_amounts
                            .get(i)
                            .ok_or_else(|| Error::ConflictError("原退款金额范围损坏".into()))?,
                    );
                }
            }
            if quantity > item.quantity.to_decimal()
                || sum_amounts(&amounts)? > item.cost_snapshot_total_gross
            {
                return Err(Error::BusinessLogicError(
                    "累计退款申请超过原订单可退数量或金额，未知申请仍占用范围".into(),
                ));
            }
        }
        Ok(())
    }

    /// 重验持久化动作与不可替换的原供应商订单身份。
    /// # 参数
    /// `order` 为调用方事务内读取的原订单。
    /// # 返回
    /// 订单、连接、供应商及外部根单均一致时成功。
    /// # 错误
    /// 已删除或身份变化时拒绝。
    pub fn ensure_order(&self, order: &SupplierFulfillmentOrder) -> Result<()> {
        if order.base.is_deleted()
            || order.base.id != self.request.order.merchant_order_no
            || order.connection_id.as_ref() != self.connection_id
            || order.supplier_id.as_ref() != self.supplier_id
            || order.external_order_no.as_deref() != Some(self.request.order.external_order_no.as_str())
        {
            return Err(Error::ConflictError("退款动作与原供应商订单身份不一致".into()));
        }
        Ok(())
    }

    /// 校验同一售后申请身份只能恢复同一冻结载荷。
    /// # 参数
    /// `candidate` 为本次重新提交的规范化请求。
    /// # 返回
    /// 原身份及载荷完全一致时成功。
    /// # 错误
    /// 同号异载荷或错配连接时拒绝。
    pub fn ensure_replay(&self, candidate: &Self) -> Result<()> {
        if self.base.id != candidate.base.id
            || self.connection_id != candidate.connection_id
            || self.request.action.payload_hash != candidate.request.action.payload_hash
        {
            return Err(Error::ConflictError("原退款申请身份已绑定不同范围或金额".into()));
        }
        Ok(())
    }

    /// 保存未支持状态；已发送动作必须保留原未知结果。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回待人工原因，停止自动调度。
    /// # 错误
    /// 无。
    pub fn unsupported(&mut self) {
        if self.state == RefundExecutionState::Ready {
            self.state = RefundExecutionState::Unsupported;
        }
        self.blocker = Some("SUPPLIER_REFUND_UNSUPPORTED: 当前协议不支持自动退款，请通过人工售后核对；已有请求结果不得视为失败".into());
        self.next_run_at = None;
    }

    /// 在持久化成功后才允许远端发送；非 Ready 不可重发。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本次是否从未发送切换为未知。
    /// # 错误
    /// 无。
    pub fn begin(&mut self) -> bool {
        if self.state != RefundExecutionState::Ready {
            return false;
        }
        self.state = RefundExecutionState::Unknown;
        self.defer("退款请求可能已发送，必须查询原动作");
        true
    }

    /// 核对最终来源收据并保存，受理或累计金额不构成完成证据。
    /// # 参数
    /// `receipt` 为按原动作查询的单笔来源证据。
    /// # 返回
    /// 合法收据进入待财务分配状态，停止查询。
    /// # 错误
    /// 原动作、订单、金额、时间或退款号不匹配时拒绝。
    pub fn receive(&mut self, receipt: Snapshot<RefundReceipt>) -> Result<()> {
        if !matches!(self.state, RefundExecutionState::Unknown | RefundExecutionState::Accepted) {
            return Err(Error::ConflictError("当前退款动作不接受自动来源结果".into()));
        }
        self.accept_receipt(receipt)
    }

    fn accept_receipt(&mut self, receipt: Snapshot<RefundReceipt>) -> Result<()> {
        let value = &receipt.value;
        if value.action_id != self.request.action.id
            || value.order != self.request.order
            || value.amount != self.request.amount
            || value.external_refund_no.trim().is_empty()
            || value.external_refund_no != value.external_refund_no.trim()
            || value.external_refund_no.len() > 128
            || value.completed_at.unix_secs() <= 0
            || value.completed_at > receipt.stamp.observed_at
        {
            return Err(Error::ConflictError("退款来源与原动作、订单、金额或业务时间不一致".into()));
        }
        self.receipt = Some(receipt);
        self.state = RefundExecutionState::Received;
        self.blocker = Some("退款来源已核实，待在退款结果登记中分配原成本、应付及现金；尚未财务过账".into());
        self.next_run_at = None;
        Ok(())
    }

    /// 对明确未发送的未支持动作登记人工核实来源，不覆盖未知远端写入。
    /// # 参数
    /// `request` 为授权结果登记命令；`actor` 为当前核实人。
    /// # 返回
    /// 原申请范围核验后进入来源已核实，保留人工凭证和操作人。
    /// # 错误
    /// 已发送、凭证缺失、来源身份或金额不匹配时拒绝。
    pub fn confirm_manual(
        &mut self,
        request: &crate::dto::supplier_fulfillment::RecordRefundResultRequest,
        actor: &str,
    ) -> Result<()> {
        let reference = request.manual_evidence_reference.as_deref().unwrap_or_default().trim();
        let version = request.external_refund_version.trim();
        if self.state != RefundExecutionState::Unsupported
            || reference.is_empty()
            || reference.len() > 512
            || version.is_empty()
            || version.len() > 128
            || actor.trim().is_empty()
            || request.source_event_id != self.base.id
        {
            return Err(Error::BusinessLogicError(
                "只有明确未发送的未支持退款可凭人工核实证据登记；未知请求必须继续调查".into(),
            ));
        }
        self.accept_receipt(Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::now(),
            },
            value: RefundReceipt {
                action_id: self.base.id.clone(),
                external_refund_no: request.external_refund_no.clone(),
                order: self.request.order.clone(),
                amount: request.refund_amount,
                completed_at: Instant::from_unix_secs(request.refunded_at),
            },
        })?;
        self.manual_evidence = Some(ManualRefundEvidence {
            reference: reference.into(),
            recorded_by: actor.into(),
            external_version: version.into(),
        });
        Ok(())
    }

    /// 校验人工财务分配必须对应已核实单笔收据及冻结范围。
    /// # 参数
    /// `request` 为既有退款结果登记命令。
    /// # 返回
    /// 返回稳定完整命令摘要，用于拒绝同收据异载荷重放。
    /// # 错误
    /// 未核实来源、金额、时间、行范围或已应用载荷不同则拒绝。
    pub fn validate_application(
        &self,
        request: &crate::dto::supplier_fulfillment::RecordRefundResultRequest,
    ) -> Result<String> {
        let receipt = self
            .receipt
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原退款尚未取得来源收据，不能登记为已退款".into()))?;
        if request.manual_evidence_reference.as_deref().map(str::trim)
            != self.manual_evidence.as_ref().map(|e| e.reference.as_str())
        {
            return Err(Error::ConflictError("退款结果的人工或接口来源与原收据不一致".into()));
        }
        if !matches!(self.state, RefundExecutionState::Received | RefundExecutionState::Applied)
            || request.external_refund_no != receipt.value.external_refund_no
            || request.refund_amount != receipt.value.amount
            || request.refunded_at != receipt.value.completed_at.unix_secs()
            || request.source_event_id != self.base.id
            || request.external_refund_version != self.receipt_version()?
            || request.allocations.len() != self.request.lines.len()
        {
            return Err(Error::ConflictError("退款登记与原来源收据不一致".into()));
        }
        let mut seen = BTreeSet::new();
        for allocation in &request.allocations {
            let index = self
                .request
                .lines
                .iter()
                .position(|line| line.line_id == allocation.supplier_fulfillment_item_id.as_ref())
                .ok_or_else(|| Error::ConflictError("退款分配不属于原申请范围".into()))?;
            if !seen.insert(allocation.supplier_fulfillment_item_id.to_string())
                || allocation.refund_quantity != self.request.lines[index].quantity
                || allocation.gross_amount != self.line_amounts[index]
            {
                return Err(Error::ConflictError("退款分配数量或金额不等于原申请".into()));
            }
        }
        let payload =
            serde_json::to_string(request).map_err(|_| Error::ValidationError("退款分配无法冻结".into()))?;
        let hash = identity(&[&payload]);
        if self.application_hash.as_ref().is_some_and(|old| old != &hash) {
            return Err(Error::ConflictError("原退款已按另一财务分配登记".into()));
        }
        Ok(hash)
    }

    /// 返回已核实收据的内容指纹，作为正式事实的不可变证据版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 稳定指纹，不冒充供应商单调版本。
    /// # 错误
    /// 未取得收据或序列化失败时拒绝。
    pub fn receipt_version(&self) -> Result<String> {
        if let Some(manual) = &self.manual_evidence {
            return Ok(manual.external_version.clone());
        }
        let receipt =
            self.receipt.as_ref().ok_or_else(|| Error::BusinessLogicError("尚未取得退款收据".into()))?;
        let payload = serde_json::to_string(&receipt.value)
            .map_err(|_| Error::ValidationError("退款收据无法冻结".into()))?;
        Ok(identity(&[&payload]))
    }

    /// 延迟只读调查，不改变发送状态。
    /// # 参数
    /// `reason` 为脱敏原因。
    /// # 返回
    /// 下次调查不早于60秒。
    /// # 错误
    /// 无。
    pub fn defer(&mut self, reason: &str) {
        self.blocker = Some(reason.into());
        self.next_run_at = Some(Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(60)));
    }
}

fn prepare_lines(
    input: &SubmitAfterSalesActionRequest,
    items: &[SupplierFulfillmentItem],
) -> Result<(Vec<ReturnLine>, Vec<Amount>)> {
    let mut inputs = input.lines.clone();
    inputs.sort_by(|a, b| a.supplier_fulfillment_item_id.cmp(&b.supplier_fulfillment_item_id));
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    let mut amounts = Vec::new();
    for line in &inputs {
        let item = items
            .iter()
            .find(|item| item.base.id == line.supplier_fulfillment_item_id.as_ref())
            .ok_or_else(|| Error::ValidationError("退款行不属于原订单".into()))?;
        if line.quantity.to_decimal() <= Amount::zero().to_decimal() || line.quantity > item.quantity {
            return Err(Error::ValidationError("退款数量超出原采购范围".into()));
        }
        let extended = item
            .unit_cost_snapshot_gross
            .to_decimal()
            .checked_mul(line.quantity.to_decimal())
            .ok_or_else(|| Error::ValidationError("退款金额溢出".into()))?;
        let maximum = Amount::try_from(round_to_cent(extended))?;
        if !seen.insert(item.base.id.clone())
            || line.quantity.to_decimal() <= Amount::zero().to_decimal()
            || line.quantity > item.quantity
            || line.amount <= Amount::zero()
            || line.amount > maximum
        {
            return Err(Error::ValidationError("退款数量或金额超出原明细范围，或存在重复行".into()));
        }
        lines.push(ReturnLine {
            line_id: item.base.id.clone(),
            sku: SupplierSku {
                product_id: item.supplier_product_code_snapshot.clone(),
                spec_id: item.supplier_sku_code_snapshot.clone(),
            },
            quantity: line.quantity,
        });
        amounts.push(line.amount);
    }
    Ok((lines, amounts))
}

fn sum_amounts(values: &[Amount]) -> Result<Amount> {
    let total = values.iter().try_fold(Amount::zero().to_decimal(), |sum, value| {
        sum.checked_add(value.to_decimal()).ok_or_else(|| Error::ValidationError("退款金额溢出".into()))
    })?;
    Ok(Amount::try_from(total)?)
}

#[cfg(test)]
mod tests {
    use std::slice;

    use erp_core::ids::{
        CostAllocationId, CostEntryId, PayableEntryId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierOfferingRevisionId,
    };

    use super::*;
    use crate::dto::supplier_fulfillment::AfterSalesActionLineRequest;
    use crate::entity::supplier_fulfillment::SupplierFulfillmentItemData;
    use crate::ports::connector::common::{SourceRevision, SourceStamp};
    fn item() -> SupplierFulfillmentItem {
        SupplierFulfillmentItem::new(
            SupplierFulfillmentItemId::new("item"),
            SupplierFulfillmentItemData::from_unit_cost(
                SupplierFulfillmentOrderId::new("order"),
                SupplierOfferingRevisionId::new("offering"),
                "sku",
                None,
                "2".parse().unwrap(),
                "10".parse().unwrap(),
                "0.13".parse().unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn input() -> SubmitAfterSalesActionRequest {
        SubmitAfterSalesActionRequest {
            request_id: None,
            lines: vec![AfterSalesActionLineRequest {
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item"),
                quantity: "1".parse().unwrap(),
                amount: "10".parse().unwrap(),
            }],
            reason_code: Some("damaged".into()),
            comment: None,
        }
    }
    fn prepare(request: &SubmitAfterSalesActionRequest) -> Result<RefundExecution> {
        RefundExecution::prepare(
            "connection",
            "supplier",
            OrderReference { merchant_order_no: "order".into(), external_order_no: "external".into() },
            request,
            &[item()],
        )
    }
    fn receipt(row: &RefundExecution) -> Snapshot<RefundReceipt> {
        Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(200),
            },
            value: RefundReceipt {
                action_id: row.base.id.clone(),
                external_refund_no: "refund-1".into(),
                order: row.request.order.clone(),
                amount: row.request.amount,
                completed_at: Instant::from_unix_secs(100),
            },
        }
    }
    #[test]
    fn scope_is_stable_and_rejects_duplicate_foreign_overquantity_and_overamount() {
        assert_eq!(prepare(&input()).unwrap().base.id, prepare(&input()).unwrap().base.id);
        for mode in 0..5 {
            let mut request = input();
            match mode {
                0 => request.lines.push(request.lines[0].clone()),
                1 => {
                    request.lines[0].supplier_fulfillment_item_id = SupplierFulfillmentItemId::new("foreign")
                },
                2 => request.lines[0].quantity = "3".parse().unwrap(),
                3 => request.lines[0].amount = "11".parse().unwrap(),
                _ => request.reason_code = None,
            }
            assert!(prepare(&request).is_err());
        }
    }
    #[test]
    fn explicit_request_identity_separates_partial_refunds_and_freezes_payload() {
        let mut first = input();
        first.request_id = Some("return-1".into());
        let one = prepare(&first).unwrap();
        let mut second = first.clone();
        second.request_id = Some("return-2".into());
        let two = prepare(&second).unwrap();
        assert_ne!(one.base.id, two.base.id);
        assert!(two.ensure_remaining(slice::from_ref(&one), &[item()]).is_ok());
        first.lines[0].amount = "9".parse().unwrap();
        let conflict = prepare(&first).unwrap();
        assert_eq!(one.base.id, conflict.base.id);
        assert_ne!(one.request.action.payload_hash, conflict.request.action.payload_hash);
        assert!(one.ensure_replay(&conflict).is_err());
        assert!(one.ensure_replay(&one).is_ok());
    }

    #[test]
    fn unknown_requests_reserve_remaining_range_and_unsupported_does_not() {
        let mut previous = prepare(&input()).unwrap();
        previous.begin();
        let mut request = input();
        request.reason_code = Some("other".into());
        request.lines[0].quantity = "2".parse().unwrap();
        request.lines[0].amount = "20".parse().unwrap();
        let next = prepare(&request).unwrap();
        assert!(next.ensure_remaining(&[previous.clone()], &[item()]).is_err());
        previous.state = RefundExecutionState::Unsupported;
        assert!(next.ensure_remaining(&[previous], &[item()]).is_ok());
    }
    #[test]
    fn unsupported_after_send_preserves_unknown_and_never_reopens_ready() {
        let mut row = prepare(&input()).unwrap();
        assert!(row.begin());
        row.unsupported();
        assert_eq!(row.state, RefundExecutionState::Unknown);
        assert!(row.next_run_at.is_none());
        assert!(!row.begin());
    }
    #[test]
    fn manual_evidence_is_only_allowed_for_proven_unsent_unsupported_actions() {
        use crate::dto::supplier_fulfillment::RecordRefundResultRequest;
        let mut row = prepare(&input()).unwrap();
        row.unsupported();
        let request = RecordRefundResultRequest {
            manual_evidence_reference: Some("evidence://verified-return-1".into()),
            external_refund_no: "refund-manual".into(),
            external_refund_version: "supplier-proof-1".into(),
            refund_amount: "10".parse().unwrap(),
            refunded_at: 100,
            source_event_id: row.base.id.clone(),
            allocations: vec![],
        };
        row.confirm_manual(&request, "operator").unwrap();
        assert_eq!(row.state, RefundExecutionState::Received);
        assert_eq!(row.manual_evidence.as_ref().unwrap().recorded_by, "operator");
        assert_eq!(row.receipt_version().unwrap(), "supplier-proof-1");
        let mut unknown = prepare(&input()).unwrap();
        unknown.begin();
        assert!(unknown.confirm_manual(&request, "operator").is_err());
        assert_eq!(unknown.state, RefundExecutionState::Unknown);
        assert!(unknown.receipt.is_none());
        let mut absent = prepare(&input()).unwrap();
        absent.unsupported();
        let mut missing = request;
        missing.manual_evidence_reference = None;
        assert!(absent.confirm_manual(&missing, "operator").is_err());
        assert!(absent.receipt.is_none());
    }

    #[test]
    fn financial_application_requires_receipt_exact_scope_and_same_replay_payload() {
        use crate::dto::supplier_fulfillment::{RecordRefundResultRequest, RefundAllocationRequest};
        let mut row = prepare(&input()).unwrap();
        row.begin();
        row.receive(receipt(&row)).unwrap();
        let mut request = RecordRefundResultRequest {
            manual_evidence_reference: None,
            external_refund_no: "refund-1".into(),
            external_refund_version: row.receipt_version().unwrap(),
            refund_amount: "10".parse().unwrap(),
            refunded_at: 100,
            source_event_id: row.base.id.clone(),
            allocations: vec![RefundAllocationRequest {
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item"),
                original_cost_entry_id: CostEntryId::new("cost"),
                original_cost_allocation_id: CostAllocationId::new("cost-allocation"),
                original_payable_entry_id: PayableEntryId::new("payable"),
                original_payment_allocation_id: None,
                refund_quantity: "1".parse().unwrap(),
                gross_amount: "10".parse().unwrap(),
                net_amount: "10".parse().unwrap(),
                tax_amount: Amount::zero(),
                payable_reduction_amount: "10".parse().unwrap(),
                cash_refund_amount: Amount::zero(),
            }],
        };
        let hash = row.validate_application(&request).unwrap();
        row.application_hash = Some(hash);
        row.state = RefundExecutionState::Applied;
        row.validate_application(&request).unwrap();
        request.allocations[0].original_cost_entry_id = CostEntryId::new("other-cost");
        assert!(row.validate_application(&request).is_err());
        row.application_hash = None;
        row.state = RefundExecutionState::Received;
        request.allocations[0].refund_quantity = "2".parse().unwrap();
        assert!(row.validate_application(&request).is_err());
        request.allocations[0].refund_quantity = "1".parse().unwrap();
        request.refund_amount = "9".parse().unwrap();
        assert!(row.validate_application(&request).is_err());
    }

    #[test]
    fn receipt_requires_exact_action_order_amount_and_source_time() {
        for mode in 0..5 {
            let mut row = prepare(&input()).unwrap();
            row.begin();
            let mut observed = receipt(&row);
            match mode {
                0 => observed.value.action_id = "foreign".into(),
                1 => observed.value.order.external_order_no = "foreign".into(),
                2 => observed.value.amount = "9".parse().unwrap(),
                3 => observed.value.completed_at = Instant::from_unix_secs(201),
                _ => observed.value.external_refund_no.clear(),
            }
            assert!(row.receive(observed).is_err());
            assert!(row.receipt.is_none());
        }
        let mut row = prepare(&input()).unwrap();
        row.begin();
        row.receive(receipt(&row)).unwrap();
        assert_eq!(row.state, RefundExecutionState::Received);
        assert!(row.fact_id.is_none());
        assert!(row.next_run_at.is_none());
        let restored: RefundExecution = serde_json::from_str(&serde_json::to_string(&row).unwrap()).unwrap();
        assert_eq!(row.receipt_version().unwrap(), restored.receipt_version().unwrap());
    }
}
