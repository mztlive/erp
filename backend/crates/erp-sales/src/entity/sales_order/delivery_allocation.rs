//! 销售行交付分配随草稿、提交及生效版本冻结，数量使用商品基础单位。
use std::fmt;

use erp_core::money::Quantity;
use erp_core::region::RegionCode;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 一条销售商品明细发往一个收货地址的数量。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesDeliveryAllocation {
    pub recipient: String,
    pub phone: String,
    /// 收货地区，必须是行政区划最末级。
    pub region_code: RegionCode,
    /// 地区完整名称快照，由服务端按代码生成。
    #[serde(default)]
    pub region_name: String,
    /// 详细地址。
    pub address: String,
    pub quantity: Quantity,
}

impl fmt::Debug for SalesDeliveryAllocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SalesDeliveryAllocation").field("quantity", &self.quantity).finish_non_exhaustive()
    }
}

impl SalesDeliveryAllocation {
    /// 规范化收货资料并校验逐行数量守恒。
    /// # 参数
    /// `allocations` 为完整分配列表；`quantity` 为销售行基础单位数量。
    /// # 返回
    /// 返回规范化列表；空列表表示尚未指定交付地址。
    /// # 错误
    /// 超过 100 个地址、资料缺失、地区不是最末级、数量非正、溢出或合计不等于行数量时拒绝。
    pub fn validate_all(mut allocations: Vec<Self>, quantity: Quantity) -> Result<Vec<Self>> {
        if allocations.is_empty() {
            return Ok(allocations);
        }
        if allocations.len() > 100 {
            return Err(Error::from("每条商品最多分配 100 个收货地址"));
        }
        let mut total = Decimal::ZERO;
        for allocation in &mut allocations {
            allocation.recipient = normalize_required_text(
                std::mem::take(&mut allocation.recipient),
                "请填写收货人",
                64,
                "收货人过长",
            )?;
            allocation.phone = normalize_required_text(
                std::mem::take(&mut allocation.phone),
                "请填写联系电话",
                32,
                "联系电话过长",
            )?;
            allocation.region_code = RegionCode::parse_leaf(allocation.region_code.as_str())?;
            allocation.region_name = allocation.region_code.full_name();
            allocation.address = normalize_required_text(
                std::mem::take(&mut allocation.address),
                "请填写详细收货地址",
                512,
                "收货地址过长",
            )?;
            if allocation.quantity.to_decimal() <= Decimal::ZERO {
                return Err(Error::from("每个地址的分配数量必须大于 0"));
            }
            total = total
                .checked_add(allocation.quantity.to_decimal())
                .ok_or_else(|| Error::from("分配数量超出范围"))?;
        }
        if total != quantity.to_decimal() {
            return Err(Error::from("各地址分配数量合计必须等于商品数量"));
        }
        Ok(allocations)
    }

    /// 返回"省市区 + 详细地址"的完整地址。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回拼接后的完整地址。
    ///
    /// # 错误
    /// 无。
    pub fn full_address(&self) -> String {
        format!("{}{}", self.region_name, self.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(quantity: &str) -> SalesDeliveryAllocation {
        SalesDeliveryAllocation {
            recipient: " 张三 ".into(),
            phone: " 13800000000 ".into(),
            region_code: RegionCode::parse("310115").unwrap(),
            region_name: String::new(),
            address: " 测试路 1 号 ".into(),
            quantity: quantity.parse().unwrap(),
        }
    }
    #[test]
    fn splits_quantities_exactly_and_normalizes_addresses() {
        let rows =
            SalesDeliveryAllocation::validate_all(vec![row("1.2"), row("2.3")], "3.5".parse().unwrap())
                .unwrap();
        assert_eq!(rows[0].recipient, "张三");
        assert_eq!(rows[0].address, "测试路 1 号");
        assert_eq!(rows[0].region_name, "上海市浦东新区");
        assert_eq!(rows[0].full_address(), "上海市浦东新区测试路 1 号");
        assert!(!format!("{:?}", rows[0]).contains("张三"));
        let json = serde_json::to_string(&rows).unwrap();
        assert_eq!(serde_json::from_str::<Vec<SalesDeliveryAllocation>>(&json).unwrap(), rows);
    }
    #[test]
    fn rejects_incomplete_overallocated_and_nonpositive_rows() {
        for amount in ["0", "1", "3"] {
            assert!(SalesDeliveryAllocation::validate_all(vec![row(amount)], "2".parse().unwrap()).is_err());
        }
        let mut missing = row("2");
        missing.address.clear();
        assert!(SalesDeliveryAllocation::validate_all(vec![missing], "2".parse().unwrap()).is_err());
        let mut city_only = row("2");
        city_only.region_code = RegionCode::parse("310100").unwrap();
        assert!(SalesDeliveryAllocation::validate_all(vec![city_only], "2".parse().unwrap()).is_err());
        assert!(SalesDeliveryAllocation::validate_all(vec![row("1"); 101], "101".parse().unwrap()).is_err());
        assert!(SalesDeliveryAllocation::validate_all(vec![], "2".parse().unwrap()).unwrap().is_empty());
    }
}
