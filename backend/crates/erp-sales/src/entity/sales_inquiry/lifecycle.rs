//! 状态迁移不接触外域；任务、审计与回执由同一事务的 Process 协调。
use std::collections::HashSet;

use erp_core::common::time::Instant;
use rust_decimal::Decimal;

use super::*;
use crate::{Error, Result};

impl SalesInquiry {
    /// 保存工作副本；已提交快照保持不变。
    /// # 参数
    /// `expected_version` 为聚合版本；`data` 为完整需求草稿；`deadline_at` 未传时保留原截止时间。
    /// # 返回
    /// 成功后原子替换草稿及传入的截止时间，已提交快照保持不变。
    /// # 错误
    /// 版本、状态、稳定身份或占用下限不满足时拒绝。
    pub fn save_draft(
        &mut self,
        expected_version: u64,
        mut data: InquiryRequirementData,
        deadline_at: Option<Instant>,
    ) -> Result<()> {
        self.ensure_version(expected_version)?;
        self.ensure_state(&[InquiryStatus::Draft, InquiryStatus::AwaitingSalesSupplement])?;
        data.normalize(false)?;
        self.validate_requirement_history(&data)?;
        self.draft = data;
        if let Some(deadline_at) = deadline_at {
            self.deadline_at = deadline_at;
        }
        Ok(())
    }

    /// 提交并追加完整冻结需求；相同需求仅开启新处理轮次。
    /// # 参数
    /// `expected_version` 为聚合版本；`actor` 为销售；`reason` 为改版原因；`now` 为提交时刻。
    /// # 返回
    /// 更新当前需求版本并直接进入采购处理中。
    /// # 错误
    /// 需求不完整、改版原因缺失、身份或来源数量下限非法时拒绝。
    pub fn submit(&mut self, expected_version: u64, actor: &str, reason: String, now: Instant) -> Result<()> {
        self.ensure_version(expected_version)?;
        self.ensure_state(&[InquiryStatus::Draft, InquiryStatus::AwaitingSalesSupplement])?;
        validate_text(actor, "提交人", 128)?;
        let mut data = self.draft.clone();
        data.normalize(true)?;
        self.validate_requirement_history(&data)?;
        if self.requirements.last().is_none_or(|last| last.data != data) {
            if self.requirement_version > 0 {
                validate_text(&reason, "需求变更原因", 2000)?;
            }
            self.append_requirement(actor, reason.clone(), data, now)?;
        }
        self.freeze_work();
        self.status = InquiryStatus::ProcurementProcessing;
        self.event(actor, "submitted", reason, now);
        Ok(())
    }

    /// 退回销售补充并冻结本轮全部采购工作。
    /// # 参数
    /// `expected_version`、`actor`、`reason`、`now` 为版本及退回事实。
    /// # 返回
    /// 进入待销售补充状态。
    /// # 错误
    /// 非采购处理中、版本过期或未填原因时拒绝。
    pub fn request_supplement(
        &mut self,
        expected_version: u64,
        actor: &str,
        reason: String,
        now: Instant,
    ) -> Result<()> {
        self.ensure_processing(expected_version)?;
        validate_text(&reason, "补充需求原因", 2000)?;
        self.freeze_work();
        self.status = InquiryStatus::AwaitingSalesSupplement;
        self.event(actor, "supplement_requested", reason, now);
        Ok(())
    }

    /// 保存当前责任行的采购工作；未发布资料不进入销售投影。
    /// # 参数
    /// `line_ids` 为事务内已授权责任范围；`data` 为该范围的工作内容。
    /// # 返回
    /// 追加当前版本工作片段并冻结本人的旧片段。
    /// # 错误
    /// 版本、状态、需求范围、重复行或越出责任范围时拒绝。
    pub fn save_feedback_work(
        &mut self,
        expected_version: u64,
        actor: &str,
        line_ids: Vec<String>,
        mut data: InquiryFeedbackData,
        now: Instant,
    ) -> Result<()> {
        self.ensure_processing(expected_version)?;
        validate_text(actor, "处理人", 128)?;
        let requirement = self.current_requirement()?;
        let unique: HashSet<_> = line_ids.iter().collect();
        if unique.len() != line_ids.len()
            || line_ids.iter().any(|id| !requirement.data.lines.iter().any(|line| &line.line_id == id))
            || data.lines.iter().any(|line| !line_ids.contains(&line.line_id))
        {
            return Err(Error::Forbidden("采购工作包含当前责任范围外的需求".into()));
        }
        self.prepare_feedback_fees(actor, &line_ids, &mut data)?;
        if data.lines.iter().any(|line| line.prices.len() > 1) {
            return Err(Error::ValidationError("每件商品最多保存一条报价".into()));
        }
        for work in &mut self.feedback_work {
            if work.actor_id == actor && !work.frozen && work.line_ids.iter().any(|id| line_ids.contains(id))
            {
                work.frozen = true;
            }
        }
        self.feedback_work.push(InquiryFeedbackWork {
            requirement_version: self.requirement_version,
            actor_id: actor.into(),
            line_ids,
            data,
            saved_at: now,
            frozen: false,
        });
        Ok(())
    }

    /// 发布完整反馈；发布前重验全部行、费用及跨版本来源余额。
    /// # 参数
    /// `data` 为合并全部责任结果后的完整白名单快照。
    /// # 返回
    /// 追加新反馈轮次并进入待销售处理。
    /// # 错误
    /// 待确认、缺失责任结果、失效报价或余额下限违反时拒绝。
    pub fn publish(
        &mut self,
        expected_version: u64,
        actor: &str,
        data: InquiryFeedbackData,
        now: Instant,
    ) -> Result<()> {
        self.ensure_processing(expected_version)?;
        data.validate(&self.current_requirement()?.data, now)?;
        if data.fees.iter().any(|fee| fee.line_ids.is_empty()) && self.shared_fee_owner_user_id.is_none() {
            return Err(Error::BusinessLogicError("整单费用须明确责任采购后发布".into()));
        }
        self.validate_feedback_floor(&data)?;
        let round =
            self.feedback_round.checked_add(1).ok_or_else(|| Error::Internal("反馈轮次超出范围".into()))?;
        self.feedbacks.push(InquiryFeedbackSnapshot {
            round,
            requirement_version: self.requirement_version,
            published_by: actor.into(),
            published_at: now,
            data,
        });
        self.feedback_round = round;
        self.freeze_work();
        self.status = InquiryStatus::AwaitingSales;
        self.event(actor, "published", String::new(), now);
        Ok(())
    }

    /// 接受当前整轮反馈；可供应部分仍由销售按需转单。
    /// # 参数
    /// `round` 为正在查看的反馈轮次。
    /// # 返回
    /// 追加接受事实并进入已确认。
    /// # 错误
    /// 版本或反馈不是当前待处理轮次时拒绝。
    pub fn accept(&mut self, expected_version: u64, actor: &str, round: u32, now: Instant) -> Result<()> {
        self.ensure_version(expected_version)?;
        self.ensure_state(&[InquiryStatus::AwaitingSales])?;
        if round != self.feedback_round {
            return Err(Error::ConflictError("反馈轮次已变化".into()));
        }
        let feedback = self
            .feedbacks
            .iter()
            .find(|feedback| feedback.round == round)
            .ok_or_else(|| Error::Internal("当前反馈快照缺失".into()))?;
        self.validate_feedback_floor(&feedback.data)?;
        self.acceptances.push(InquiryAcceptance {
            requirement_version: self.requirement_version,
            feedback_round: round,
            accepted_by: actor.into(),
            accepted_at: now,
        });
        self.status = InquiryStatus::Confirmed;
        self.event(actor, "accepted", String::new(), now);
        Ok(())
    }

    /// 对未转部分重询，需求变化时冻结新需求版本。
    /// # 参数
    /// `data` 包含原因、稳定范围及可选完整新版需求。
    /// # 返回
    /// 追加重询记录并进入待采购处理；既有来源保持不变。
    /// # 错误
    /// 状态、范围、原因、需求身份或占用下限非法时拒绝。
    pub fn reinquire(
        &mut self,
        expected_version: u64,
        actor: &str,
        mut data: InquiryReinquiryData,
        now: Instant,
    ) -> Result<()> {
        self.ensure_version(expected_version)?;
        self.ensure_state(&[InquiryStatus::AwaitingSales, InquiryStatus::Confirmed])?;
        validate_text(&data.reason, "重询原因", 2000)?;
        self.validate_reinquiry_scope(&data)?;
        if let Some(requirement) = &mut data.requirement {
            requirement.normalize(true)?;
            self.validate_requirement_history(requirement)?;
            if *requirement != self.current_requirement()?.data {
                self.append_requirement(actor, data.reason.clone(), requirement.clone(), now)?;
            }
            self.draft = requirement.clone();
        }
        self.reinquiries.push(InquiryReinquiry {
            initiated_by_procurement: false,
            actor_id: actor.into(),
            at: now,
            previous_feedback_round: self.feedback_round,
            requirement_version: self.requirement_version,
            data,
        });
        self.freeze_work();
        self.status = InquiryStatus::ProcurementProcessing;
        Ok(())
    }

    /// 采购主动开启整单新报价，保留既有需求、接受及来源台账。
    /// # 参数
    /// `expected_version` 为聚合版本，`actor`、`reason`、`now` 为本次报价发起事实。
    /// # 返回
    /// 冻结旧工作并进入采购处理中；发布时再生成新的反馈轮次。
    /// # 错误
    /// 版本冲突、尚在处理中、已关闭或原因缺失时拒绝。
    pub fn renew_quote(
        &mut self,
        expected_version: u64,
        actor: &str,
        reason: String,
        now: Instant,
    ) -> Result<()> {
        self.ensure_version(expected_version)?;
        self.ensure_state(&[InquiryStatus::AwaitingSales, InquiryStatus::Confirmed])?;
        validate_text(actor, "报价发起人", 128)?;
        validate_text(&reason, "新一轮报价原因", 2000)?;
        self.reinquire(
            expected_version,
            actor,
            InquiryReinquiryData {
                reason: reason.clone(),
                line_ids: vec![],
                fee_ids: vec![],
                concerns: "采购主动更新整单报价".into(),
                requirement: None,
            },
            now,
        )?;
        self.reinquiries.last_mut().expect("成功重询已追加本轮记录").initiated_by_procurement = true;
        self.event(actor, "quote_renewed", reason, now);
        Ok(())
    }

    /// 关闭询价并冻结工作资料，已转销售采购仍按自身生命周期运行。
    /// # 参数
    /// `reason` 为必填关闭原因，其余为版本及操作事实。
    /// # 返回
    /// 状态改为已关闭。
    /// # 错误
    /// 已关闭、版本过期或原因缺失时拒绝。
    pub fn close(&mut self, expected_version: u64, actor: &str, reason: String, now: Instant) -> Result<()> {
        self.ensure_version(expected_version)?;
        if self.status == InquiryStatus::Closed {
            return Err(Error::BusinessLogicError("询价已关闭".into()));
        }
        validate_text(&reason, "关闭原因", 2000)?;
        self.freeze_work();
        self.status = InquiryStatus::Closed;
        self.event(actor, "closed", reason, now);
        Ok(())
    }

    fn append_requirement(
        &mut self,
        actor: &str,
        reason: String,
        data: InquiryRequirementData,
        now: Instant,
    ) -> Result<()> {
        let version = self
            .requirement_version
            .checked_add(1)
            .ok_or_else(|| Error::Internal("需求版本超出范围".into()))?;
        self.requirements.push(InquiryRequirementSnapshot {
            version,
            submitted_by: actor.into(),
            submitted_at: now,
            change_reason: reason,
            data,
        });
        self.requirement_version = version;
        Ok(())
    }

    fn freeze_work(&mut self) {
        for work in &mut self.feedback_work {
            work.frozen = true;
        }
    }

    fn validate_reinquiry_scope(&self, data: &InquiryReinquiryData) -> Result<()> {
        let demand = &self.current_requirement()?.data;
        let feedback =
            self.feedbacks.last().ok_or_else(|| Error::BusinessLogicError("尚无反馈可重询".into()))?;
        if data.line_ids.iter().any(|id| !demand.lines.iter().any(|line| &line.line_id == id))
            || data.fee_ids.iter().any(|id| !feedback.data.fees.iter().any(|fee| &fee.fee_id == id))
        {
            return Err(Error::ValidationError("重询范围包含未知商品或费用".into()));
        }
        Ok(())
    }

    pub(super) fn validate_requirement_history(&self, data: &InquiryRequirementData) -> Result<()> {
        self.validate_stable_identities(data)?;
        for reservation in
            self.quantity_ledger.iter().filter(|entry| entry.state != InquiryReservationState::Released)
        {
            for selection in &reservation.selections {
                let line = data
                    .lines
                    .iter()
                    .find(|line| line.line_id == selection.line_id)
                    .ok_or_else(|| Error::BusinessLogicError("已有来源占用的需求行不得删除".into()))?;
                if self.used_quantity(&line.line_id, None, None)? > line.quantity.to_decimal() {
                    return Err(Error::BusinessLogicError("新需求数量低于已占用及已消耗数量".into()));
                }
                if let Some(id) = &selection.allocation_id {
                    let allocation =
                        line.allocations.iter().find(|row| &row.allocation_id == id).ok_or_else(|| {
                            Error::BusinessLogicError("已有来源占用的配送地址不得删除".into())
                        })?;
                    if self.used_quantity(&line.line_id, Some(id), None)? > allocation.quantity.to_decimal() {
                        return Err(Error::BusinessLogicError("新地址数量低于已占用及已消耗数量".into()));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_stable_identities(&self, data: &InquiryRequirementData) -> Result<()> {
        for old in self.requirements.iter().flat_map(|snapshot| &snapshot.data.lines) {
            if data.lines.iter().any(|line| line.line_id == old.line_id && line.sku_id != old.sku_id) {
                return Err(Error::ValidationError("更换 SKU 必须建立新的需求行身份".into()));
            }
            for previous in &old.allocations {
                for line in &data.lines {
                    let Some(allocation) =
                        line.allocations.iter().find(|row| row.allocation_id == previous.allocation_id)
                    else {
                        continue;
                    };
                    if line.line_id != old.line_id
                        || previous.recipient != allocation.recipient
                        || previous.phone != allocation.phone
                        || previous.region_code != allocation.region_code
                        || previous.address != allocation.address
                    {
                        return Err(Error::ValidationError(
                            "更换需求行或收件地址必须建立新的配送身份".into(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_feedback_floor(&self, data: &InquiryFeedbackData) -> Result<()> {
        for demand in &self.current_requirement()?.data.lines {
            let line = data
                .lines
                .iter()
                .find(|line| line.line_id == demand.line_id)
                .ok_or_else(|| Error::ValidationError("反馈缺少需求行".into()))?;
            if self.used_quantity(&line.line_id, None, None)? > line.available_quantity.to_decimal() {
                return Err(Error::BusinessLogicError("可供量不得低于已占用及已消耗量".into()));
            }
            for allocation in &demand.allocations {
                let offered = allocation.quantity.to_decimal();
                if self.used_quantity(&line.line_id, Some(&allocation.allocation_id), None)? > offered {
                    return Err(Error::BusinessLogicError("地址可供量不得低于已占用及已消耗量".into()));
                }
            }
        }
        for old in self.feedbacks.iter().flat_map(|snapshot| &snapshot.data.fees) {
            let used = self.used_fee(&old.fee_id, None)?;
            let allocated = sum_decimal(
                self.fee_allocations
                    .iter()
                    .filter(|row| row.fee_id == old.fee_id)
                    .map(|row| row.amount.to_decimal()),
            )?;
            if used == Decimal::ZERO && allocated == Decimal::ZERO {
                continue;
            }
            let current = data
                .fees
                .iter()
                .find(|fee| fee.fee_id == old.fee_id)
                .ok_or_else(|| Error::BusinessLogicError("已分摊或已占用费用不得删除".into()))?;
            if current.amount_gross.to_decimal() < used.max(allocated) || old.line_ids != current.line_ids {
                return Err(Error::BusinessLogicError("费用新结果低于来源占用或改写已有办理身份".into()));
            }
        }
        Ok(())
    }
}
