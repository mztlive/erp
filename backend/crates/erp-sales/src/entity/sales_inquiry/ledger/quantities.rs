//! 销售草稿占用、正式消耗及安全减少。
use erp_core::common::time::Instant;
use rust_decimal::Decimal;

use super::super::{
    InquiryFeedbackLine, InquiryFeedbackSnapshot, InquiryQuantityReservation, InquiryQuantitySelection,
    InquiryReservationState, InquiryStatus, InquirySupplyPrice, SalesInquiry, sum_decimal, validate_text,
};
use crate::{Error, Result};

impl SalesInquiry {
    /// 取得当前已接受的反馈；逐行有效期仍须在选择时校验。
    /// # 参数
    /// `round` 为明确引用轮次；`now` 为操作时刻。
    /// # 返回
    /// 返回需求版本及接受事实一致的原反馈。
    /// # 错误
    /// 已关闭、未接受、被新结果替代或需求版本改变时拒绝。
    pub fn accepted_feedback(&self, round: u32, _now: Instant) -> Result<&InquiryFeedbackSnapshot> {
        if self.status == InquiryStatus::Closed
            || self.acceptances.last().is_none_or(|entry| {
                entry.feedback_round != round || entry.requirement_version != self.requirement_version
            })
        {
            return Err(Error::BusinessLogicError("只能引用当前已接受且未关闭的询价结果".into()));
        }
        self.feedbacks
            .iter()
            .find(|feedback| {
                feedback.round == round && feedback.requirement_version == self.requirement_version
            })
            .ok_or_else(|| Error::BusinessLogicError("需求已变更，必须接受新反馈".into()))
    }

    /// 原子占用已接受反馈的商品及配送数量。
    /// # 参数
    /// `reservation_id` 和 `sales_order_id` 为调用方生成的稳定单据身份；`selections` 为本次来源数量。
    /// # 返回
    /// 追加未消耗占用；完全相同重放无新增。
    /// # 错误
    /// 过期、重询范围、价格条件、不供货、行/地址或分段余额不足时拒绝。
    pub fn reserve_quantities(
        &mut self,
        reservation_id: &str,
        sales_order_id: &str,
        round: u32,
        selections: Vec<InquiryQuantitySelection>,
        now: Instant,
    ) -> Result<()> {
        validate_text(reservation_id, "来源占用身份", 128)?;
        validate_text(sales_order_id, "销售单身份", 128)?;
        if let Some(existing) =
            self.quantity_ledger.iter().find(|entry| entry.reservation_id == reservation_id)
        {
            if existing.sales_order_id == sales_order_id
                && existing.feedback_round == round
                && existing.selections == selections
                && existing.state != InquiryReservationState::Released
            {
                return Ok(());
            }
            return Err(Error::ConflictError("来源占用身份已用于其他内容".into()));
        }
        self.validate_quantity_selection(round, &selections, None, now)?;
        self.quantity_ledger.push(InquiryQuantityReservation {
            reservation_id: reservation_id.into(),
            sales_order_id: sales_order_id.into(),
            requirement_version: self.requirement_version,
            feedback_round: round,
            selections,
            state: InquiryReservationState::Reserved,
        });
        Ok(())
    }

    /// 调整既有草稿来源；减少不要求报价仍有效。
    /// # 参数
    /// `reservation_id` 为原占用；`selections` 为新的完整来源数量。
    /// # 返回
    /// 替换占用；空选择释放全部未消耗占用。
    /// # 错误
    /// 已正式消耗不能直接修改；增加或变更地址时重新校验当前结果。
    pub fn reconcile_quantities(
        &mut self,
        reservation_id: &str,
        selections: Vec<InquiryQuantitySelection>,
        now: Instant,
    ) -> Result<()> {
        let index = self
            .quantity_ledger
            .iter()
            .position(|entry| entry.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("询价来源占用".into()))?;
        let old = &self.quantity_ledger[index];
        if old.state != InquiryReservationState::Reserved {
            return Err(Error::BusinessLogicError("只有销售草稿来源占用允许调整".into()));
        }
        self.validate_reserved_precision(old.requirement_version, &selections)?;
        if !is_reduction(&old.selections, &selections)? {
            self.validate_quantity_selection(old.feedback_round, &selections, Some(reservation_id), now)?;
        }
        self.quantity_ledger[index].selections = selections;
        if self.quantity_ledger[index].selections.is_empty() {
            self.quantity_ledger[index].state = InquiryReservationState::Released;
        }
        Ok(())
    }

    /// 将已保留来源转为正式消耗，不重复扣减余额。
    /// # 参数
    /// `reservation_id` 为销售单创建时的来源占用。
    /// # 返回
    /// Reserved 变为 Handled；重复调用保持不变。
    /// # 错误
    /// 未找到或已经释放时拒绝；询价关闭、过期或重询不影响既有销售生效。
    pub fn consume_quantities(&mut self, reservation_id: &str) -> Result<()> {
        let entry = self
            .quantity_ledger
            .iter_mut()
            .find(|entry| entry.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("询价来源占用".into()))?;
        if entry.state == InquiryReservationState::Released {
            return Err(Error::BusinessLogicError("已释放来源不能消耗".into()));
        }
        entry.state = InquiryReservationState::Handled;
        Ok(())
    }

    /// 释放尚未生效的销售草稿占用。
    /// # 参数
    /// `reservation_id` 为原来源占用。
    /// # 返回
    /// 已释放重放无新增；未消耗来源恢复可用余额。
    /// # 错误
    /// 已正式消耗不能凭状态直接释放，必须执行正式销售变更规则。
    pub fn release_quantities(&mut self, reservation_id: &str) -> Result<()> {
        let entry = self
            .quantity_ledger
            .iter_mut()
            .find(|entry| entry.reservation_id == reservation_id)
            .ok_or_else(|| Error::NotFound("询价来源占用".into()))?;
        if entry.state == InquiryReservationState::Handled {
            return Err(Error::BusinessLogicError("正式消耗必须依据有效销售变更处理".into()));
        }
        entry.state = InquiryReservationState::Released;
        Ok(())
    }

    pub(in super::super) fn used_quantity(
        &self,
        line_id: &str,
        allocation_id: Option<&str>,
        exclude: Option<&str>,
    ) -> Result<Decimal> {
        sum_decimal(
            self.quantity_ledger
                .iter()
                .filter(|entry| {
                    entry.state != InquiryReservationState::Released
                        && Some(entry.reservation_id.as_str()) != exclude
                })
                .flat_map(|entry| &entry.selections)
                .filter(|row| {
                    row.line_id == line_id
                        && allocation_id.is_none_or(|id| row.allocation_id.as_deref() == Some(id))
                })
                .map(|row| row.quantity.to_decimal()),
        )
    }

    fn validate_quantity_selection(
        &self,
        round: u32,
        selections: &[InquiryQuantitySelection],
        exclude: Option<&str>,
        now: Instant,
    ) -> Result<()> {
        if selections.is_empty() || selections.len() > 20_000 {
            return Err(Error::ValidationError("转单来源数量不能为空或超过限制".into()));
        }
        let feedback = self.accepted_feedback(round, now)?;
        for selection in selections {
            if selection.quantity.to_decimal() <= Decimal::ZERO {
                return Err(Error::ValidationError("转单数量必须大于零".into()));
            }
            self.ensure_line_not_reinquiring(&selection.line_id)?;
            let line = feedback
                .data
                .lines
                .iter()
                .find(|row| row.line_id == selection.line_id)
                .ok_or_else(|| Error::ValidationError("转单行不属于已接受反馈".into()))?;
            let total = selected_quantity(selections, &selection.line_id, None)?;
            if total
                .checked_add(self.used_quantity(&selection.line_id, None, exclude)?)
                .is_none_or(|sum| sum > line.available_quantity.to_decimal())
            {
                return Err(Error::BusinessLogicError("商品可转数量不足".into()));
            }
            if selection.price_id.is_none() && line.prices.len() != 1 {
                return Err(Error::ValidationError("多个供给价格段必须明确价格身份".into()));
            }
            self.validate_selection_precision(selection)?;
            let price = line
                .prices
                .iter()
                .find(|price| {
                    selection.price_id.as_ref().is_none_or(|id| &price.price_id == id)
                        && selection
                            .allocation_id
                            .as_ref()
                            .is_none_or(|id| price.allocations.iter().any(|row| &row.allocation_id == id))
                })
                .ok_or_else(|| Error::BusinessLogicError("转单数量没有适用价格或配送范围".into()))?;
            if price.valid_until <= now {
                return Err(Error::BusinessLogicError("询价供给价格已过期".into()));
            }
            self.validate_allocation_selection(selections, selection, line, price, exclude)?;
            self.validate_price_selection(selections, selection, price, exclude)?;
        }
        Ok(())
    }

    fn validate_reserved_precision(
        &self,
        requirement_version: u32,
        selections: &[InquiryQuantitySelection],
    ) -> Result<()> {
        let requirement = self
            .requirements
            .iter()
            .find(|snapshot| snapshot.version == requirement_version)
            .ok_or_else(|| Error::Internal("来源原需求快照缺失".into()))?;
        for selection in selections {
            let line = requirement
                .data
                .lines
                .iter()
                .find(|line| line.line_id == selection.line_id)
                .ok_or_else(|| Error::ValidationError("来源行不属于原需求版本".into()))?;
            line.validate_supply_quantity(selection.quantity)?;
        }
        Ok(())
    }

    fn validate_selection_precision(&self, selection: &InquiryQuantitySelection) -> Result<()> {
        let demand = self
            .current_requirement()?
            .data
            .lines
            .iter()
            .find(|row| row.line_id == selection.line_id)
            .ok_or_else(|| Error::ValidationError("需求行不存在".into()))?;
        if selection.quantity.to_decimal().normalize().scale() > demand.quantity_scale {
            return Err(Error::ValidationError("转单数量不符合商品单位精度".into()));
        }
        Ok(())
    }

    fn validate_allocation_selection(
        &self,
        selections: &[InquiryQuantitySelection],
        selection: &InquiryQuantitySelection,
        line: &InquiryFeedbackLine,
        price: &InquirySupplyPrice,
        exclude: Option<&str>,
    ) -> Result<()> {
        let Some(id) = &selection.allocation_id else {
            return if price.allocations.is_empty() {
                Ok(())
            } else {
                Err(Error::ValidationError("实物转单必须明确配送分配身份".into()))
            };
        };
        let offered = sum_decimal(
            line.prices
                .iter()
                .flat_map(|price| &price.allocations)
                .filter(|row| &row.allocation_id == id)
                .map(|row| row.quantity.to_decimal()),
        )?;
        let selected = selected_quantity(selections, &selection.line_id, Some(id))?;
        if selected
            .checked_add(self.used_quantity(&selection.line_id, Some(id), exclude)?)
            .is_none_or(|sum| sum > offered)
        {
            return Err(Error::BusinessLogicError("配送地址可转数量不足".into()));
        }
        let price_offered = price
            .allocations
            .iter()
            .find(|row| &row.allocation_id == id)
            .ok_or_else(|| Error::BusinessLogicError("所选价格不覆盖该配送地址".into()))?
            .quantity
            .to_decimal();
        let matches = |row: &&InquiryQuantitySelection| {
            row.line_id == selection.line_id
                && row.allocation_id == selection.allocation_id
                && row.price_id == selection.price_id
        };
        let price_selected =
            sum_decimal(selections.iter().filter(matches).map(|row| row.quantity.to_decimal()))?;
        let price_used = sum_decimal(
            self.quantity_ledger
                .iter()
                .filter(|entry| {
                    entry.state != InquiryReservationState::Released
                        && Some(entry.reservation_id.as_str()) != exclude
                })
                .flat_map(|entry| &entry.selections)
                .filter(matches)
                .map(|row| row.quantity.to_decimal()),
        )?;
        if price_selected.checked_add(price_used).is_none_or(|sum| sum > price_offered) {
            return Err(Error::BusinessLogicError("所选价格的地址可转数量不足".into()));
        }
        Ok(())
    }

    fn validate_price_selection(
        &self,
        selections: &[InquiryQuantitySelection],
        selection: &InquiryQuantitySelection,
        price: &InquirySupplyPrice,
        exclude: Option<&str>,
    ) -> Result<()> {
        let selected = sum_decimal(
            selections
                .iter()
                .filter(|row| row.line_id == selection.line_id && row.price_id == selection.price_id)
                .map(|row| row.quantity.to_decimal()),
        )?;
        let previous = sum_decimal(
            self.quantity_ledger
                .iter()
                .filter(|entry| {
                    entry.state != InquiryReservationState::Released
                        && Some(entry.reservation_id.as_str()) != exclude
                })
                .flat_map(|entry| &entry.selections)
                .filter(|row| row.line_id == selection.line_id && row.price_id == selection.price_id)
                .map(|row| row.quantity.to_decimal()),
        )?;
        let total =
            selected.checked_add(previous).ok_or_else(|| Error::ValidationError("来源数量溢出".into()))?;
        let threshold = if price.allow_cumulative { total } else { selected };
        if threshold < price.min_quantity.to_decimal()
            || price.max_quantity.is_some_and(|max| threshold > max.to_decimal())
            || total > price.quantity.to_decimal()
        {
            return Err(Error::BusinessLogicError("分批数量不满足报价数量档位或价格段余额".into()));
        }
        Ok(())
    }

    /// 核对当前接受之后尚未接受的重询范围，不以新发布反馈替代接受事实。
    /// # 参数
    /// `line_id` 为稳定需求行，调用前须验证接受轮次与需求版本。
    /// # 返回
    /// 已确认或所有未接受重询均不影响该行时通过。
    /// # 错误
    /// 没有可证明的接受范围、整单重询或该行受影响时拒绝。
    pub(crate) fn ensure_line_not_reinquiring(&self, line_id: &str) -> Result<()> {
        if self.status == InquiryStatus::Confirmed {
            return Ok(());
        }
        let round = self.acceptances.last().map(|entry| entry.feedback_round);
        let (has_active, affected) = self
            .reinquiries
            .iter()
            .filter(|entry| round.is_some_and(|round| entry.previous_feedback_round >= round))
            .fold((false, false), |(_, affected), entry| {
                (
                    true,
                    affected
                        || (entry.data.line_ids.is_empty() && entry.data.fee_ids.is_empty())
                        || entry.data.line_ids.iter().any(|id| id == line_id),
                )
            });
        if !has_active || affected {
            return Err(Error::BusinessLogicError("本范围正在重新询价，暂停新增转单".into()));
        }
        Ok(())
    }
}

fn selected_quantity(
    selections: &[InquiryQuantitySelection],
    line_id: &str,
    allocation_id: Option<&str>,
) -> Result<Decimal> {
    sum_decimal(
        selections
            .iter()
            .filter(|row| {
                row.line_id == line_id
                    && allocation_id.is_none_or(|id| row.allocation_id.as_deref() == Some(id))
            })
            .map(|row| row.quantity.to_decimal()),
    )
}

fn is_reduction(old: &[InquiryQuantitySelection], new: &[InquiryQuantitySelection]) -> Result<bool> {
    for row in new {
        if row.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::ValidationError("来源数量必须为正".into()));
        }
        let matches = |candidate: &&InquiryQuantitySelection| {
            candidate.line_id == row.line_id
                && candidate.allocation_id == row.allocation_id
                && candidate.price_id == row.price_id
        };
        let previous = sum_decimal(old.iter().filter(matches).map(|entry| entry.quantity.to_decimal()))?;
        let current = sum_decimal(new.iter().filter(matches).map(|entry| entry.quantity.to_decimal()))?;
        if current > previous {
            return Ok(false);
        }
    }
    Ok(true)
}
