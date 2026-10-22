//! 专项报价跨销售和采购的累计采用台账；采用记录与采购同事务保存。
use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::money::Quantity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{InquiryQuote, InquiryQuoteUse, QuoteAddressLimit};
use crate::{Error, Result};

/// 同一稳定报价跨修订共享余额，防止新修订重置供应数量。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct InquiryQuoteBalance {
    #[serde(flatten)]
    pub base: BaseModel,
    pub reserved_quantity: Quantity,
    pub address_quantities: Vec<QuoteAddressLimit>,
}
/// 与具体采购行绑定的不可变采用；原单历史与通用供给来源保持分离。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct InquiryQuoteReservation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub purchase_order_id: String,
    pub released: bool,
    pub quote_id: String,
    pub quote_revision_id: String,
    pub quantity: Quantity,
    pub addresses: Vec<QuoteAddressLimit>,
    #[serde(default)]
    pub history: Vec<QuoteReservationHistory>,
}
/// 每次正式采用调整前的不可变数量与地址记录。
#[derive(Clone, Serialize, Deserialize)]
pub struct QuoteReservationHistory {
    pub quantity: Quantity,
    pub addresses: Vec<QuoteAddressLimit>,
}
impl InquiryQuoteBalance {
    /// 返回原条款下尚未采用的逐地址上限，历史采用不因新候选重置。
    /// # 参数
    /// `quote`为本稳定报价准确修订。
    /// # 返回
    /// 返回剩余大于零的原地址范围。
    /// # 错误
    /// 数量账损坏或归属不符时拒绝。
    pub fn remaining_addresses(&self, quote: &InquiryQuote) -> Result<Vec<QuoteAddressLimit>> {
        validate_quantity_rows(self.reserved_quantity, &self.address_quantities, false)?;
        if self.base.id != quote.data.quote_id {
            return Err(Error::ConflictError("专项报价余额归属不一致".into()));
        }
        quote
            .data
            .terms
            .address_limits
            .iter()
            .filter_map(|limit| {
                let consumed = self
                    .address_quantities
                    .iter()
                    .find(|row| {
                        row.allocation_id == limit.allocation_id
                            && row.address_fingerprint == limit.address_fingerprint
                    })
                    .map(|row| row.quantity.to_decimal())
                    .unwrap_or(Decimal::ZERO);
                let value = limit
                    .quantity
                    .to_decimal()
                    .checked_sub(consumed)
                    .unwrap_or(Decimal::ZERO)
                    .max(Decimal::ZERO);
                if value.is_zero() {
                    return None;
                }
                Some(
                    Quantity::try_from(value)
                        .map(|quantity| QuoteAddressLimit {
                            allocation_id: limit.allocation_id.clone(),
                            address_fingerprint: limit.address_fingerprint.clone(),
                            quantity,
                        })
                        .map_err(Into::into),
                )
            })
            .collect()
    }

    /// 按正式作废事实释放尚未履行的采用量，保留原采用记录。
    /// # 参数
    /// `reservation` 为当前未释放的正式采购采用。
    /// # 返回
    /// 更新内存余额；重复释放由调用方按released标记跳过。
    /// # 错误
    /// 余额不足、地址不匹配或累计账损坏时拒绝。
    pub fn release(&mut self, reservation: &InquiryQuoteReservation) -> Result<()> {
        validate_quantity_rows(self.reserved_quantity, &self.address_quantities, false)?;
        validate_quantity_rows(reservation.quantity, &reservation.addresses, true)?;
        if self.base.id != reservation.quote_id || reservation.released {
            return Err(Error::ConflictError("专项报价释放依据无效".into()));
        }
        let remaining = self
            .reserved_quantity
            .to_decimal()
            .checked_sub(reservation.quantity.to_decimal())
            .filter(|value| *value >= Decimal::ZERO)
            .ok_or_else(|| Error::ConflictError("专项报价采用余额损坏".into()))?;
        let mut addresses = self.address_quantities.clone();
        for requested in &reservation.addresses {
            let row = addresses
                .iter_mut()
                .find(|row| {
                    row.allocation_id == requested.allocation_id
                        && row.address_fingerprint == requested.address_fingerprint
                })
                .ok_or_else(|| Error::ConflictError("专项报价地址采用记录缺失".into()))?;
            let value = row
                .quantity
                .to_decimal()
                .checked_sub(requested.quantity.to_decimal())
                .filter(|value| *value >= Decimal::ZERO)
                .ok_or_else(|| Error::ConflictError("专项报价地址采用余额损坏".into()))?;
            row.quantity = Quantity::try_from(value)?;
        }
        addresses.retain(|row| !row.quantity.to_decimal().is_zero());
        self.reserved_quantity = Quantity::try_from(remaining)?;
        self.address_quantities = addresses;
        Ok(())
    }
    /// 按既有正式采用缩减待生效最大保留量，不重新取得任何采购资格。
    /// # 参数
    /// `reservation`为同一原采用；目标数量及地址必须完全包含在已保留范围内。
    /// # 返回
    /// 返回保留历史的新采用，仅减少当前余额。
    /// # 错误
    /// 缺失、已释放、增量、新地址、超原地址量或账损坏时拒绝。
    pub fn restore_existing_usage(
        &mut self,
        reservation: &InquiryQuoteReservation,
        quantity: Quantity,
        addresses: &[QuoteAddressLimit],
    ) -> Result<InquiryQuoteReservation> {
        validate_quantity_rows(reservation.quantity, &reservation.addresses, true)?;
        validate_quantity_rows(quantity, addresses, true)?;
        if reservation.released
            || reservation.quote_id != self.base.id
            || quantity > reservation.quantity
            || reservation.addresses.is_empty() != addresses.is_empty()
        {
            return Err(Error::ConflictError("仅可恢复同一已保留专项采用，不得增加数量或新地址".into()));
        }
        if addresses.iter().any(|row| {
            !reservation.addresses.iter().any(|prior| {
                prior.allocation_id == row.allocation_id
                    && prior.address_fingerprint == row.address_fingerprint
                    && prior.quantity >= row.quantity
            })
        }) {
            return Err(Error::ConflictError("恢复专项采用的地址不属于原保留范围".into()));
        }
        let mut result = reservation.clone();
        if quantity == result.quantity && addresses == result.addresses {
            return Ok(result);
        }
        let delta = Quantity::try_from(
            result
                .quantity
                .to_decimal()
                .checked_sub(quantity.to_decimal())
                .ok_or_else(|| Error::ConflictError("专项采用恢复数量损坏".into()))?,
        )?;
        if delta.to_decimal().is_zero() {
            return Ok(result);
        }
        let mut released = result.clone();
        released.quantity = delta;
        released.addresses = restoration_difference(&result.addresses, addresses)?;
        self.release(&released)?;
        result
            .history
            .push(QuoteReservationHistory { quantity: result.quantity, addresses: result.addresses.clone() });
        result.quantity = quantity;
        result.addresses = addresses.to_vec();
        Ok(result)
    }

    /// 在同一余额中替换采用，审批中保留原数量和地址的最大暴露范围。
    /// # 参数
    /// `quote`和`usage`为准确条款与新目标，`reservation`为原采用，`keep_existing`控制审批中保留旧范围。
    /// # 返回
    /// 返回更新后的采用并保留旧数量历史；失败不修改当前余额。
    /// # 错误
    /// 条件失效、MOQ或累计余额不足时拒绝。
    pub fn resized(
        &mut self,
        quote: &InquiryQuote,
        usage: &InquiryQuoteUse,
        reservation: &InquiryQuoteReservation,
        keep_existing: bool,
    ) -> Result<InquiryQuoteReservation> {
        quote.ensure_applicable(usage)?;
        let mut next = self.clone();
        next.release(reservation)?;
        let mut target = usage.clone();
        if keep_existing {
            target = held_usage(target, reservation)?;
        }
        next.reserve(quote, &target)?;
        let mut result = reservation.clone();
        if result.quantity != target.quantity || result.addresses != target.addresses {
            result.history.push(QuoteReservationHistory {
                quantity: result.quantity,
                addresses: result.addresses.clone(),
            });
            result.quantity = target.quantity;
            result.addresses = target.addresses;
        }
        *self = next;
        Ok(result)
    }
    pub(super) fn empty(id: String) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id),
            reserved_quantity: Quantity::try_from(Decimal::ZERO)?,
            address_quantities: vec![],
        })
    }
    /// 以当前CAS余额核对并登记本次采购，跨版本和销售共享供应限额。
    /// # 参数
    /// `quote` 为精确报价，`usage` 为本次经服务器核验的采购分配。
    /// # 返回
    /// 更新内存余额；持久化由调用方在采购事务内执行。
    /// # 错误
    /// 供量、地址累计或本次MOQ不符合时拒绝，失败不修改余额。
    pub fn reserve(&mut self, quote: &InquiryQuote, usage: &InquiryQuoteUse) -> Result<()> {
        validate_quantity_rows(self.reserved_quantity, &self.address_quantities, false)?;
        if self.base.id != quote.data.quote_id {
            return Err(Error::ConflictError("专项报价余额归属不一致".into()));
        }
        let mut usage = usage.clone();
        usage.consumed_quantity = self.reserved_quantity;
        quote.ensure_applicable(&usage)?;
        let mut next = self.address_quantities.clone();
        for address in &usage.addresses {
            let prior = next.iter_mut().find(|row| {
                row.allocation_id == address.allocation_id
                    && row.address_fingerprint == address.address_fingerprint
            });
            match prior {
                Some(row) => row.quantity = add(row.quantity, address.quantity)?,
                None => next.push(address.clone()),
            }
        }
        for address in &next {
            if !quote.data.terms.address_limits.iter().any(|limit| {
                limit.allocation_id == address.allocation_id
                    && limit.address_fingerprint == address.address_fingerprint
                    && limit.quantity >= address.quantity
            }) {
                return Err(Error::BusinessLogicError("专项报价同一地址累计采用超量或地址已变化".into()));
            }
        }
        let total = add(self.reserved_quantity, usage.quantity)?;
        self.address_quantities = next;
        self.reserved_quantity = total;
        Ok(())
    }
}
fn add(left: Quantity, right: Quantity) -> Result<Quantity> {
    Ok(Quantity::try_from(
        left.to_decimal()
            .checked_add(right.to_decimal())
            .ok_or_else(|| Error::ValidationError("专项报价采用数量超出范围".into()))?,
    )?)
}

fn held_usage(mut usage: InquiryQuoteUse, reservation: &InquiryQuoteReservation) -> Result<InquiryQuoteUse> {
    for prior in &reservation.addresses {
        if let Some(row) = usage.addresses.iter_mut().find(|row| {
            row.allocation_id == prior.allocation_id && row.address_fingerprint == prior.address_fingerprint
        }) {
            row.quantity = row.quantity.max(prior.quantity);
        } else {
            usage.addresses.push(prior.clone());
        }
    }
    usage.quantity = if usage.addresses.is_empty() {
        usage.quantity.max(reservation.quantity)
    } else {
        Quantity::try_from(usage.addresses.iter().try_fold(Decimal::ZERO, |total, row| {
            total
                .checked_add(row.quantity.to_decimal())
                .ok_or_else(|| Error::ValidationError("专项报价待生效范围超出限制".into()))
        })?)?
    };
    Ok(usage)
}

fn validate_quantity_rows(quantity: Quantity, rows: &[QuoteAddressLimit], positive: bool) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut total = Decimal::ZERO;
    if quantity.to_decimal() < Decimal::ZERO || (positive && quantity.to_decimal().is_zero()) {
        return Err(Error::ConflictError("专项报价采用数量损坏".into()));
    }
    for row in rows {
        if row.quantity.to_decimal() <= Decimal::ZERO || !seen.insert(&row.allocation_id) {
            return Err(Error::ConflictError("专项报价采用地址数量损坏".into()));
        }
        total = total
            .checked_add(row.quantity.to_decimal())
            .ok_or_else(|| Error::ConflictError("专项报价采用地址数量溢出".into()))?;
    }
    if !rows.is_empty() && total != quantity.to_decimal() {
        return Err(Error::ConflictError("专项报价采用地址数量不守恒".into()));
    }
    Ok(())
}

fn restoration_difference(
    prior: &[QuoteAddressLimit],
    target: &[QuoteAddressLimit],
) -> Result<Vec<QuoteAddressLimit>> {
    prior
        .iter()
        .filter_map(|old| {
            let new = target
                .iter()
                .find(|row| {
                    row.allocation_id == old.allocation_id
                        && row.address_fingerprint == old.address_fingerprint
                })
                .map(|row| row.quantity.to_decimal())
                .unwrap_or(Decimal::ZERO);
            let delta = old.quantity.to_decimal() - new;
            if delta.is_zero() {
                return None;
            }
            Some(
                Quantity::try_from(delta)
                    .map(|quantity| QuoteAddressLimit {
                        allocation_id: old.allocation_id.clone(),
                        address_fingerprint: old.address_fingerprint.clone(),
                        quantity,
                    })
                    .map_err(Into::into),
            )
        })
        .collect()
}
