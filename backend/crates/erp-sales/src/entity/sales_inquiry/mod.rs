//! 询价需求、已发布反馈、销售接受及跨版本来源余额。

mod demand;
mod feedback;
mod ledger;
mod lifecycle;
mod receipt;
mod types;
mod work;

pub use demand::*;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
pub use feedback::*;
pub use ledger::*;
pub use receipt::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
pub use types::*;

use crate::{Error, Result};

/// 询价聚合；快照仅追加，数量与费用台账使用跨版本稳定来源身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SalesInquiry {
    #[serde(flatten)]
    pub base: BaseModel,
    pub inquiry_no: String,
    pub customer_id: String,
    pub customer_name: String,
    pub sales_owner_user_id: String,
    pub business_org_unit_id: String,
    pub deadline_at: Instant,
    pub copied_from_inquiry_id: Option<String>,
    pub status: InquiryStatus,
    pub requirement_version: u32,
    pub feedback_round: u32,
    #[serde(default)]
    pub shared_fee_owner_user_id: Option<String>,
    pub draft: InquiryRequirementData,
    pub requirements: Vec<InquiryRequirementSnapshot>,
    pub feedbacks: Vec<InquiryFeedbackSnapshot>,
    pub feedback_work: Vec<InquiryFeedbackWork>,
    pub fee_allocations: Vec<InquiryFeeAllocation>,
    pub fee_allocation_history: Vec<InquiryFeeAllocation>,
    pub acceptances: Vec<InquiryAcceptance>,
    pub reinquiries: Vec<InquiryReinquiry>,
    pub events: Vec<InquiryEvent>,
    pub quantity_ledger: Vec<InquiryQuantityReservation>,
    pub fee_ledger: Vec<InquiryFeeReservation>,
}

impl SalesInquiry {
    /// 创建只包含可编辑需求的询价。
    /// # 参数
    /// `id`、`inquiry_no` 为稳定身份；`data` 为已授权的客户和负责人资料。
    /// # 返回
    /// 返回尚未产生任何业务副作用的草稿。
    /// # 错误
    /// 身份、负责人、客户或需求非法时拒绝。
    pub fn create(id: String, inquiry_no: String, mut data: InquiryCreateData) -> Result<Self> {
        validate_text(&id, "询价身份", 128)?;
        validate_text(&inquiry_no, "询价单号", 64)?;
        validate_text(&data.customer_id, "客户", 128)?;
        validate_text(&data.customer_name, "客户名称", 256)?;
        validate_text(&data.sales_owner_user_id, "销售负责人", 128)?;
        validate_text(&data.business_org_unit_id, "业务组织", 128)?;
        data.draft.normalize(false)?;
        Ok(Self {
            base: BaseModel::new(id),
            inquiry_no,
            customer_id: data.customer_id,
            customer_name: data.customer_name,
            sales_owner_user_id: data.sales_owner_user_id,
            business_org_unit_id: data.business_org_unit_id,
            deadline_at: data.deadline_at,
            copied_from_inquiry_id: data.copied_from_inquiry_id,
            status: InquiryStatus::Draft,
            requirement_version: 0,
            feedback_round: 0,
            shared_fee_owner_user_id: None,
            draft: data.draft,
            requirements: vec![],
            feedbacks: vec![],
            feedback_work: vec![],
            fee_allocations: vec![],
            fee_allocation_history: vec![],
            acceptances: vec![],
            reinquiries: vec![],
            events: vec![],
            quantity_ledger: vec![],
            fee_ledger: vec![],
        })
    }

    /// 核对写入持有的聚合版本。
    /// # 参数
    /// `expected_version` 为读取或编辑时的版本。
    /// # 返回
    /// 版本匹配且未删除时通过；持久化时仍须执行 CAS。
    /// # 错误
    /// 版本过期、已删除时返回冲突。
    pub fn ensure_version(&self, expected_version: u64) -> Result<()> {
        if self.base.is_deleted() || self.base.version != expected_version {
            return Err(Error::ConflictError("询价已发生变化，请刷新后重试".into()));
        }
        Ok(())
    }

    /// 读取当前冻结的需求。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回当前需求快照。
    /// # 错误
    /// 尚未提交时拒绝。
    pub fn current_requirement(&self) -> Result<&InquiryRequirementSnapshot> {
        self.requirements.last().ok_or_else(|| Error::BusinessLogicError("询价需求尚未提交".into()))
    }

    /// 核对内部报价维护要求的当前版本和处理状态。
    /// # 参数
    /// `expected_version` 为调用方持有的聚合版本。
    /// # 返回
    /// 当前采购处理中且版本一致时通过。
    /// # 错误
    /// 版本过期或状态不允许时拒绝。
    pub fn ensure_processing(&self, expected_version: u64) -> Result<()> {
        self.ensure_version(expected_version)?;
        // 历史待采购记录与已分配任务同属处理阶段，无需额外接手。
        self.ensure_state(&[InquiryStatus::AwaitingProcurement, InquiryStatus::ProcurementProcessing])
    }

    /// 读取尚在采购处理阶段的冻结需求，残留任务不得取得关闭后的授权。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回当前有效采购任务对应需求。
    /// # 错误
    /// 非待采购/采购处理中或尚无冻结需求时拒绝。
    pub fn current_procurement_requirement(&self) -> Result<&InquiryRequirementSnapshot> {
        self.ensure_state(&[InquiryStatus::AwaitingProcurement, InquiryStatus::ProcurementProcessing])?;
        self.current_requirement()
    }

    /// 读取当前责任范围内的单条需求。
    /// # 参数
    /// `scope` 为已重验任务责任行；`line_id` 为请求行。
    /// # 返回
    /// 返回该行冻结需求。
    /// # 错误
    /// 状态、范围或需求版本不满足时拒绝。
    pub fn responsibility_line(&self, scope: &[String], line_id: &str) -> Result<&InquiryRequirementLine> {
        if !scope.iter().any(|id| id == line_id) {
            return Err(Error::Forbidden("内部报价超出当前责任行".into()));
        }
        self.current_procurement_requirement()?
            .data
            .lines
            .iter()
            .find(|line| line.line_id == line_id)
            .ok_or_else(|| Error::ConflictError("询价需求行已变化".into()))
    }

    /// 读取当前责任采购自己保存的未发布费用。
    /// # 参数
    /// `actor`、`scope` 为当前处理人和任务责任；`fee_id` 为稳定费用身份。
    /// # 返回
    /// 返回当前工作片段中的费用。
    /// # 错误
    /// 非处理阶段、缺少当前工作或费用越权时拒绝。
    pub fn responsibility_fee(&self, actor: &str, scope: &[String], fee_id: &str) -> Result<&InquiryFee> {
        self.current_procurement_requirement()?;
        let work = self
            .feedback_work
            .iter()
            .rev()
            .find(|work| {
                !work.frozen
                    && work.requirement_version == self.requirement_version
                    && work.actor_id == actor
                    && work.line_ids == scope
            })
            .ok_or_else(|| Error::BusinessLogicError("请先保存当前责任反馈及费用".into()))?;
        work.data
            .fees
            .iter()
            .find(|fee| fee.fee_id == fee_id)
            .ok_or_else(|| Error::Forbidden("费用不属于当前责任采购工作".into()))
    }

    fn ensure_state(&self, states: &[InquiryStatus]) -> Result<()> {
        if !states.contains(&self.status) {
            return Err(Error::BusinessLogicError("当前询价状态不允许此操作".into()));
        }
        Ok(())
    }

    fn event(&mut self, actor: &str, kind: &str, reason: String, now: Instant) {
        self.events.push(InquiryEvent { actor_id: actor.to_string(), kind: kind.into(), reason, at: now });
    }
}

pub(super) fn validate_text(value: &str, label: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > max {
        return Err(Error::ValidationError(format!("{label}不能为空且不得超过 {max} 字符")));
    }
    Ok(())
}

pub(super) fn sum_decimal(mut values: impl Iterator<Item = Decimal>) -> Result<Decimal> {
    values.try_fold(Decimal::ZERO, |sum, value| {
        sum.checked_add(value).ok_or_else(|| Error::ValidationError("数量或金额超出范围".into()))
    })
}

#[cfg(test)]
mod tests;
