use erp_core::common::calendar::CalendarPeriod;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use super::{CooperationApplication, CooperationDecision, CooperationStatus};
use crate::{Error, ReconciliationCycle, Result, SettlementMode, SupplierPaymentTerm};

/// 门户合作条款允许列表；供应商身份由服务器会话绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CooperationRequest {
    pub expected_supplier_version: u64,
    pub expected_profile_id: String,
    pub settlement_mode: SettlementMode,
    pub reconciliation_cycle: ReconciliationCycle,
    pub payment_term: String,
    pub reason: String,
}

impl CooperationRequest {
    /// 规范化合作条款并校验固定付款规则。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回规范化后可冻结的申请内容。
    /// # 错误
    /// 版本为零、目标为空、付款规则不一致或原因非法时拒绝。
    pub fn normalized(mut self) -> Result<Self> {
        if self.expected_supplier_version == 0 {
            return Err(Error::ValidationError("供应商版本不能为空".into()));
        }
        self.expected_profile_id = normalize_required_text(
            self.expected_profile_id,
            "当前商务版本不能为空",
            128,
            "当前商务版本过长",
        )?;
        self.reason = normalize_required_text(self.reason, "申请原因不能为空", 500, "申请原因过长")?;
        let term = SupplierPaymentTerm::parse(&self.payment_term)?;
        if term.settlement_mode() != self.settlement_mode {
            return Err(Error::ValidationError("付款条件与结算方式不一致".into()));
        }
        if let Some((period, _)) = term.calendar_due() {
            let cycle = match period {
                CalendarPeriod::Week => ReconciliationCycle::Weekly,
                CalendarPeriod::Month => ReconciliationCycle::Monthly,
                CalendarPeriod::Quarter => ReconciliationCycle::Quarterly,
                CalendarPeriod::HalfYear => ReconciliationCycle::HalfYearly,
                CalendarPeriod::Year => ReconciliationCycle::Yearly,
            };
            if self.reconciliation_cycle != cycle {
                return Err(Error::ValidationError("对账周期与自然结算周期不一致".into()));
            }
        }
        self.payment_term = term.code();
        Ok(self)
    }
}

/// 内部确认提交后的正式商务结果；独立于原供应商提交事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CooperationResult {
    pub profile_id: String,
    pub profile_revision_no: u32,
    pub supplier_version: u64,
    pub confirmed_by: String,
    pub confirmed_at: u64,
}

/// 门户最小申请视图；不包含内部主体、银行、评估或任务备注。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CooperationView {
    pub id: String,
    pub version: u64,
    pub status: CooperationStatus,
    pub proposal: CooperationRequest,
    pub submissions: Vec<CooperationSubmissionView>,
    pub decisions: Vec<CooperationDecision>,
    pub result: Option<CooperationResult>,
}

/// 供应商可见提交历史；内部任务及处理人标识不进入外部响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CooperationSubmissionView {
    pub submission_no: u32,
    pub proposal: CooperationRequest,
    pub submitted_by: String,
    pub submitted_at: u64,
}

impl CooperationView {
    /// 将本域申请转换为供应商可见视图。
    ///
    /// # 参数
    /// * `value` - 已完成供应商范围授权的申请。
    /// # 返回
    /// 返回允许列表字段。
    /// # 错误
    /// 无。
    pub fn from_application(value: &CooperationApplication) -> Self {
        Self {
            id: value.base.id.clone(),
            version: value.base.version,
            status: value.status,
            proposal: value.proposal.clone(),
            submissions: value
                .submissions
                .iter()
                .map(|submission| CooperationSubmissionView {
                    submission_no: submission.submission_no,
                    proposal: submission.proposal.clone(),
                    submitted_by: submission.submitted_by.clone(),
                    submitted_at: submission.submitted_at,
                })
                .collect(),
            decisions: value.decisions.clone(),
            result: value.result.clone(),
        }
    }
}
