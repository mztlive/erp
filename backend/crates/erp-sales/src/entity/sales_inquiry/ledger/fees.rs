//! 费用明确分摊及采购/财务来源占用。
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use rust_decimal::Decimal;

use super::super::{
    InquiryFee, InquiryFeeAllocation, InquiryFeeReservation, InquiryReservationState, InquiryStatus,
    SalesInquiry, sum_decimal, validate_text,
};
use crate::{Error, Result};

impl SalesInquiry {
    /// 对一张销售明确分摊已接受的费用金额。
    /// # 参数
    /// `allocation_id` 是稳定分摊身份，其余参数指定销售、轮次、费用及金额。
    /// # 返回
    /// 追加分摊，完全相同重放不重复登记。
    /// # 错误
    /// 未接受、过期、范围或金额超出费用总额时拒绝。
    pub fn allocate_fee(
        &mut self,
        allocation_id: &str,
        sales_order_id: &str,
        round: u32,
        fee_id: &str,
        amount: Amount,
        now: Instant,
    ) -> Result<()> {
        validate_text(allocation_id, "费用分摊身份", 128)?;
        validate_text(sales_order_id, "销售单身份", 128)?;
        if let Some(index) = self.fee_allocations.iter().position(|row| row.allocation_id == allocation_id) {
            return self.resize_fee_allocation(index, sales_order_id, round, fee_id, amount, now);
        }
        let fee = self.accepted_fee(round, fee_id, now)?;
        let previous = sum_decimal(
            self.fee_allocations.iter().filter(|row| row.fee_id == fee_id).map(|row| row.amount.to_decimal()),
        )?;
        if amount.to_decimal() <= Decimal::ZERO
            || previous
                .checked_add(amount.to_decimal())
                .is_none_or(|total| total > fee.amount_gross.to_decimal())
        {
            return Err(Error::BusinessLogicError("费用分摊合计超过来源金额".into()));
        }
        self.fee_allocations.push(InquiryFeeAllocation {
            allocation_id: allocation_id.into(),
            sales_order_id: sales_order_id.into(),
            feedback_round: round,
            fee_id: fee_id.into(),
            amount,
        });
        Ok(())
    }

    /// 占用明确销售归属下的费用，用于采购或财务直接登记。
    /// # 参数
    /// `reservation` 必须包含稳定分摊及实际单据/行身份；`now` 为操作时刻。
    /// # 返回
    /// 写入 Reserved 来源占用；完全相同重放无新增。
    /// # 错误
    /// 暂估、含价、来源身份重复、未分摊或余额不足时拒绝。
    pub fn reserve_fee(&mut self, reservation: InquiryFeeReservation, now: Instant) -> Result<()> {
        for (value, name) in [
            (&reservation.reservation_id, "费用占用身份"),
            (&reservation.source_document_id, "费用办理单据"),
            (&reservation.source_line_id, "费用办理明细"),
        ] {
            validate_text(value, name, 128)?;
        }
        if let Some(existing) =
            self.fee_ledger.iter().find(|row| row.reservation_id == reservation.reservation_id)
        {
            if existing == &reservation {
                return Ok(());
            }
            return Err(Error::ConflictError("费用来源占用已用于其他内容".into()));
        }
        if reservation.state != InquiryReservationState::Reserved
            || self.fee_ledger.iter().any(|row| {
                row.source_document_id == reservation.source_document_id
                    && row.source_line_id == reservation.source_line_id
                    && row.state != InquiryReservationState::Released
            })
        {
            return Err(Error::ConflictError("费用办理明细已登记来源".into()));
        }
        self.validate_fee_reservation(&reservation, None, now)?;
        self.fee_ledger.push(reservation);
        Ok(())
    }

    /// 调整未发生费用占用；减少允许在询价关闭或报价过期后执行。
    /// # 参数
    /// `reservation_id` 为原来源；`amount` 为完整新金额；`now` 为操作时刻。
    /// # 返回
    /// 成功后修改保留额度，金额为零时释放。
    /// # 错误
    /// 已发生/财务登记不允许直接调整；增额须重验最新接受与余额。
    pub fn adjust_fee(&mut self, reservation_id: &str, amount: Amount, now: Instant) -> Result<()> {
        let round = self
            .fee_ledger
            .iter()
            .find(|row| row.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("费用来源占用".into()))?
            .feedback_round;
        self.adjust_fee_for_round(reservation_id, amount, round, now)
    }

    /// 使用明确已接受轮次调整来源占用，保留原正式单据引用由调用方冻结。
    /// # 参数
    /// `reservation_id` 为稳定占用；`amount` 为目标金额；`round` 为明确采用轮次；`now` 为操作时刻。
    /// # 返回
    /// 更新金额、采用轮次及占用状态；已释放零额来源可显式重新占用。
    /// # 错误
    /// 已发生来源的任何实质修改被拒绝；增加或换轮必须重新校验当前接受与余额。
    pub fn adjust_fee_for_round(
        &mut self,
        reservation_id: &str,
        amount: Amount,
        round: u32,
        now: Instant,
    ) -> Result<()> {
        let index = self
            .fee_ledger
            .iter()
            .position(|row| row.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("费用来源占用".into()))?;
        let mut adjusted = self.fee_ledger[index].clone();
        if adjusted.amount == amount
            && adjusted.feedback_round == round
            && (adjusted.state != InquiryReservationState::Released || amount.to_decimal() == Decimal::ZERO)
        {
            return Ok(());
        }
        if adjusted.state == InquiryReservationState::Handled || amount.to_decimal() < Decimal::ZERO {
            return Err(Error::BusinessLogicError("已发生费用或负金额不能调整".into()));
        }
        let increased = amount > adjusted.amount || adjusted.state == InquiryReservationState::Released;
        let changed_round = round != adjusted.feedback_round;
        adjusted.amount = amount;
        adjusted.feedback_round = round;
        if (increased || changed_round) && amount.to_decimal() > Decimal::ZERO {
            self.validate_fee_reservation(&adjusted, Some(reservation_id), now)?;
        } else if changed_round {
            self.accepted_fee(round, &adjusted.fee_id, now)?;
        }
        adjusted.state = if amount.to_decimal() == Decimal::ZERO {
            InquiryReservationState::Released
        } else {
            InquiryReservationState::Reserved
        };
        self.fee_ledger[index] = adjusted;
        Ok(())
    }

    /// 将实际发生或财务登记的费用保留为不可直接释放事实。
    /// # 参数
    /// `reservation_id` 为费用来源占用。
    /// # 返回
    /// 进入 Handled，重复调用不重复扣减。
    /// # 错误
    /// 不存在或已释放的来源不得登记。
    pub fn register_fee(&mut self, reservation_id: &str) -> Result<()> {
        let entry = self
            .fee_ledger
            .iter_mut()
            .find(|row| row.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("费用来源占用".into()))?;
        if entry.state == InquiryReservationState::Released {
            return Err(Error::BusinessLogicError("已释放费用来源不能登记发生".into()));
        }
        entry.state = InquiryReservationState::Handled;
        Ok(())
    }

    /// 依据正式作废/减少业务事实释放未发生费用。
    /// # 参数
    /// `reservation_id` 为原占用；业务资格由调用方 Process 校验。
    /// # 返回
    /// 进入 Released，重复释放保持不变。
    /// # 错误
    /// 已发生、已付款或已登记来源不得直接释放。
    pub fn release_fee(&mut self, reservation_id: &str) -> Result<()> {
        let entry = self
            .fee_ledger
            .iter_mut()
            .find(|row| row.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("费用来源占用".into()))?;
        if entry.state == InquiryReservationState::Handled {
            return Err(Error::BusinessLogicError("已发生或已登记费用不能直接释放".into()));
        }
        entry.state = InquiryReservationState::Released;
        Ok(())
    }

    pub(in super::super) fn used_fee(&self, fee_id: &str, exclude: Option<&str>) -> Result<Decimal> {
        sum_decimal(
            self.fee_ledger
                .iter()
                .filter(|row| {
                    row.fee_id == fee_id
                        && row.state != InquiryReservationState::Released
                        && Some(row.reservation_id.as_str()) != exclude
                })
                .map(|row| row.amount.to_decimal()),
        )
    }

    fn resize_fee_allocation(
        &mut self,
        index: usize,
        sales_order_id: &str,
        round: u32,
        fee_id: &str,
        amount: Amount,
        now: Instant,
    ) -> Result<()> {
        let existing = self.fee_allocations[index].clone();
        if existing.sales_order_id != sales_order_id || existing.fee_id != fee_id {
            return Err(Error::ConflictError("费用分摊身份不得改换销售或来源费用".into()));
        }
        if existing.feedback_round == round && existing.amount == amount {
            return Ok(());
        }
        let used = sum_decimal(
            self.fee_ledger
                .iter()
                .filter(|row| {
                    row.allocation_id == existing.allocation_id
                        && row.state != InquiryReservationState::Released
                })
                .map(|row| row.amount.to_decimal()),
        )?;
        if amount.to_decimal() < used || amount.to_decimal() < Decimal::ZERO {
            return Err(Error::BusinessLogicError("费用分摊不得低于已有有效占用及办理金额".into()));
        }
        if round != existing.feedback_round {
            self.accepted_fee(round, fee_id, now)?;
        }
        if amount > existing.amount {
            let fee = self.accepted_fee(round, fee_id, now)?;
            let other = sum_decimal(
                self.fee_allocations
                    .iter()
                    .enumerate()
                    .filter(|(candidate, row)| *candidate != index && row.fee_id == fee_id)
                    .map(|(_, row)| row.amount.to_decimal()),
            )?;
            if other
                .checked_add(amount.to_decimal())
                .is_none_or(|total| total > fee.amount_gross.to_decimal())
            {
                return Err(Error::BusinessLogicError("费用分摊合计超过当前已接受金额".into()));
            }
        }
        self.fee_allocation_history.push(existing);
        self.fee_allocations[index].amount = amount;
        self.fee_allocations[index].feedback_round = round;
        Ok(())
    }

    /// 核对费用公开结果是否可用于新增办理。
    /// # 参数
    /// `round`、`fee_id` 为明确公开来源；`now` 为当前时刻。
    /// # 返回
    /// 当前已接受、有效、金额大于零时通过。
    /// # 错误
    /// 未接受、过期、重询、关闭、零额时返回公开业务原因。
    pub fn ensure_fee_available(&self, round: u32, fee_id: &str, now: Instant) -> Result<()> {
        let fee = self.accepted_fee(round, fee_id, now)?;
        if fee.amount_gross.to_decimal() <= Decimal::ZERO {
            return Err(Error::BusinessLogicError("零额费用不能新增办理".into()));
        }
        Ok(())
    }

    /// 计算当前有效费用和明确销售分摊的共同可办理余额。
    /// # 参数
    /// `fee_id`、`allocation_id` 为稳定来源；`round` 为当前接受轮次；`now` 为业务时刻。
    /// # 返回
    /// 返回费用余额与分摊余额的较小值，已占用及已办理均扣除，耗尽返回零。
    /// # 错误
    /// 未接受、失效、暂估、含价、分摊不匹配或金额溢出时拒绝。
    pub fn fee_available_amount(
        &self,
        fee_id: &str,
        allocation_id: &str,
        round: u32,
        now: Instant,
    ) -> Result<Amount> {
        self.ensure_fee_available(round, fee_id, now)?;
        let fee = self.accepted_fee(round, fee_id, now)?;
        let allocation = self
            .fee_allocations
            .iter()
            .find(|row| row.allocation_id == allocation_id && row.fee_id == fee_id)
            .ok_or_else(|| Error::BusinessLogicError("费用尚无有效销售金额归属".into()))?;
        let used = sum_decimal(
            self.fee_ledger
                .iter()
                .filter(|row| {
                    row.allocation_id == allocation_id && row.state != InquiryReservationState::Released
                })
                .map(|row| row.amount.to_decimal()),
        )?;
        let fee_remaining = fee
            .amount_gross
            .to_decimal()
            .checked_sub(self.used_fee(fee_id, None)?)
            .ok_or_else(|| Error::ValidationError("费用余额计算溢出".into()))?;
        let allocation_remaining = allocation
            .amount
            .to_decimal()
            .checked_sub(used)
            .ok_or_else(|| Error::ValidationError("费用分摊余额计算溢出".into()))?;
        Ok(Amount::try_from(fee_remaining.min(allocation_remaining).max(Decimal::ZERO))?)
    }

    fn accepted_fee(&self, round: u32, fee_id: &str, now: Instant) -> Result<&InquiryFee> {
        let feedback = self.accepted_feedback(round, now)?;
        let fee = feedback
            .data
            .fees
            .iter()
            .find(|row| row.fee_id == fee_id)
            .ok_or_else(|| Error::NotFound("已接受费用".into()))?;
        if fee.valid_until <= now {
            return Err(Error::BusinessLogicError("费用结果已过期".into()));
        }
        if self.status != InquiryStatus::Confirmed {
            let (has_active, affected) = self
                .reinquiries
                .iter()
                .filter(|entry| entry.previous_feedback_round >= round)
                .fold((false, false), |(_, affected), entry| {
                    (
                        true,
                        affected
                            || (entry.data.line_ids.is_empty() && entry.data.fee_ids.is_empty())
                            || entry.data.fee_ids.iter().any(|id| id == fee_id),
                    )
                });
            if !has_active || affected {
                return Err(Error::BusinessLogicError("费用正在重新询价".into()));
            }
        }
        Ok(fee)
    }

    fn validate_fee_reservation(
        &self,
        reservation: &InquiryFeeReservation,
        exclude: Option<&str>,
        now: Instant,
    ) -> Result<()> {
        let fee = self.accepted_fee(reservation.feedback_round, &reservation.fee_id, now)?;
        let allocation = self
            .fee_allocations
            .iter()
            .find(|row| {
                row.allocation_id == reservation.allocation_id
                    && row.sales_order_id == reservation.sales_order_id
                    && row.fee_id == reservation.fee_id
            })
            .ok_or_else(|| Error::BusinessLogicError("须先明确费用对销售单的金额分摊".into()))?;
        let allocated_used = sum_decimal(
            self.fee_ledger
                .iter()
                .filter(|row| {
                    row.allocation_id == allocation.allocation_id
                        && row.state != InquiryReservationState::Released
                        && Some(row.reservation_id.as_str()) != exclude
                })
                .map(|row| row.amount.to_decimal()),
        )?;
        let amount = reservation.amount.to_decimal();
        if amount <= Decimal::ZERO
            || allocated_used.checked_add(amount).is_none_or(|sum| sum > allocation.amount.to_decimal())
            || self
                .used_fee(&reservation.fee_id, exclude)?
                .checked_add(amount)
                .is_none_or(|sum| sum > fee.amount_gross.to_decimal())
        {
            return Err(Error::BusinessLogicError("费用来源或销售分摊可用余额不足".into()));
        }
        Ok(())
    }
}
