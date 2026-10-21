//! 稳定需求行及配送身份；快照保留商品与地区显示值。
use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_core::money::{Quantity, UnitPrice};
use erp_core::region::RegionCode;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{sum_decimal, validate_text};
use crate::{Error, Result};

pub const MAX_INQUIRY_LINES: usize = 200;
pub const MAX_INQUIRY_ALLOCATIONS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryFulfillmentKind {
    Physical,
    Virtual,
    Service,
}

/// 询价草稿及需求快照共用结构，不含任何供应商字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct InquiryRequirementData {
    pub lines: Vec<InquiryRequirementLine>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryRequirementLine {
    pub line_id: String,
    pub sku_id: String,
    pub sku_revision_id: String,
    pub sku_code: String,
    pub product_name: String,
    pub specification: String,
    pub unit_code: String,
    pub quantity_scale: u32,
    pub quantity: Quantity,
    pub fulfillment_kind: InquiryFulfillmentKind,
    pub delivery_due_at: Instant,
    #[serde(default)]
    pub delivery_terms: String,
    #[serde(default)]
    pub sales_reference_price_gross: Option<UnitPrice>,
    #[serde(default)]
    pub allocations: Vec<InquiryDeliveryAllocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryDeliveryAllocation {
    pub allocation_id: String,
    pub recipient: String,
    pub phone: String,
    pub region_code: RegionCode,
    pub region_name: String,
    pub address: String,
    pub quantity: Quantity,
}

impl InquiryRequirementData {
    /// 规范化需求并校验稳定身份、单位精度和数量守恒。
    /// # 参数
    /// `submitted` 为真时要求至少一行且实物地址完整。
    /// # 返回
    /// 原位保留规范化资料和服务端地区名称快照。
    /// # 错误
    /// 行数、身份、数量、地址、金额、单位精度非法时拒绝。
    pub fn normalize(&mut self, submitted: bool) -> Result<()> {
        if self.lines.len() > MAX_INQUIRY_LINES || (submitted && self.lines.is_empty()) {
            return Err(Error::ValidationError("询价须包含 1 至 200 条商品行".into()));
        }
        if self.note.chars().count() > 4000 {
            return Err(Error::ValidationError("需求说明过长".into()));
        }
        let mut line_ids = HashSet::new();
        let mut allocation_ids = HashSet::new();
        for line in &mut self.lines {
            line.normalize(submitted)?;
            if !line_ids.insert(line.line_id.clone()) {
                return Err(Error::ValidationError("需求行身份重复".into()));
            }
            for allocation in &line.allocations {
                if !allocation_ids.insert(allocation.allocation_id.clone()) {
                    return Err(Error::ValidationError("配送分配身份重复".into()));
                }
            }
        }
        Ok(())
    }
}

impl InquiryRequirementLine {
    /// 校验供给量符合需求商品的基础单位精度。
    /// # 参数
    /// `quantity` 为报价起订量、可供量或地址限额；不限制其与本次总需求的大小关系。
    /// # 返回
    /// 严格正数且单位精度合法时通过。
    /// # 错误
    /// 非正数或单位精度不符时拒绝。
    pub fn validate_supply_quantity(&self, quantity: Quantity) -> Result<()> {
        if quantity.to_decimal() <= Decimal::ZERO
            || quantity.to_decimal().normalize().scale() > self.quantity_scale
        {
            return Err(Error::ValidationError("供给数量必须为正且符合商品单位精度".into()));
        }
        Ok(())
    }

    /// 校验询价专项报价的商品总量与明确配送适用范围。
    /// # 参数
    /// `available_quantity` 为专项可供总量；`allocation_ids` 为条款明确覆盖的稳定地址身份。
    /// # 返回
    /// 数量符合单位精度、不超过本行需求且配送范围有效时通过。
    /// # 错误
    /// 超量、实物地址范围为空或引用其他需求地址时拒绝。
    pub fn validate_quote_scope(
        &self,
        available_quantity: Quantity,
        allocation_ids: &[String],
    ) -> Result<()> {
        self.validate_supply_quantity(available_quantity)?;
        if available_quantity > self.quantity {
            return Err(Error::ValidationError("专项报价可供量不得超过本行询价需求".into()));
        }
        if self.fulfillment_kind == InquiryFulfillmentKind::Physical && allocation_ids.is_empty() {
            return Err(Error::ValidationError("实物专项报价必须明确配送地址范围".into()));
        }
        if allocation_ids
            .iter()
            .any(|id| !self.allocations.iter().any(|allocation| &allocation.allocation_id == id))
        {
            return Err(Error::ValidationError("专项报价配送地址不属于本行询价需求".into()));
        }
        Ok(())
    }

    fn normalize(&mut self, submitted: bool) -> Result<()> {
        for (value, label) in [
            (&self.line_id, "需求行身份"),
            (&self.sku_id, "SKU"),
            (&self.sku_revision_id, "SKU版本"),
            (&self.sku_code, "SKU编码"),
            (&self.product_name, "商品名称"),
            (&self.unit_code, "单位"),
        ] {
            validate_text(value, label, 256)?;
        }
        if self.unit_code.chars().count() > 32 {
            return Err(Error::ValidationError("单位编码过长".into()));
        }
        if self.specification.chars().count() > 256 || self.delivery_terms.chars().count() > 2000 {
            return Err(Error::ValidationError("商品规格或交付条件过长".into()));
        }
        if self.quantity_scale > 6
            || self.quantity.to_decimal() <= Decimal::ZERO
            || self.quantity.to_decimal().normalize().scale() > self.quantity_scale
        {
            return Err(Error::ValidationError("商品数量必须为正且符合单位精度".into()));
        }
        if self.sales_reference_price_gross.is_some_and(|price| price.to_decimal() < Decimal::ZERO) {
            return Err(Error::ValidationError("销售参考价不得为负数".into()));
        }
        if self.allocations.len() > MAX_INQUIRY_ALLOCATIONS {
            return Err(Error::ValidationError("每条商品最多分配 100 个收货地址".into()));
        }
        if submitted
            && self.fulfillment_kind == InquiryFulfillmentKind::Physical
            && self.allocations.is_empty()
        {
            return Err(Error::ValidationError("实物商品提交前必须补齐配送地址".into()));
        }
        for allocation in &mut self.allocations {
            allocation.normalize(self.quantity_scale)?;
        }
        if !self.allocations.is_empty()
            && sum_decimal(self.allocations.iter().map(|row| row.quantity.to_decimal()))?
                != self.quantity.to_decimal()
        {
            return Err(Error::ValidationError("地址数量合计必须等于需求数量".into()));
        }
        self.unit_code = self.unit_code.trim().to_string();
        Ok(())
    }
}

impl InquiryDeliveryAllocation {
    fn normalize(&mut self, quantity_scale: u32) -> Result<()> {
        validate_text(&self.allocation_id, "配送分配身份", 128)?;
        for (value, label, max) in [
            (&mut self.recipient, "收货人", 64),
            (&mut self.phone, "联系电话", 32),
            (&mut self.address, "详细地址", 512),
        ] {
            *value = value.trim().to_string();
            validate_text(value, label, max)?;
        }
        self.region_code = RegionCode::parse_leaf(self.region_code.as_str())?;
        self.region_name = self.region_code.full_name();
        if self.quantity.to_decimal() <= Decimal::ZERO
            || self.quantity.to_decimal().normalize().scale() > quantity_scale
        {
            return Err(Error::ValidationError("地址数量必须为正且符合单位精度".into()));
        }
        Ok(())
    }
}
