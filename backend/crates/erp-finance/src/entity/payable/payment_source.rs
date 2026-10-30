//! 核销资金来源；银行付款与供应商预存款使用各自保留真实身份。

use erp_core::ids::SupplierPaymentId;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::de::Error as DeserializeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 正式付款核销的唯一资金来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentAllocationSource {
    /// 原供应商付款；历史记录继续读取原字段。
    SupplierPayment(SupplierPaymentId),
    /// 已核实扣款形成的供应商预存款使用，不是第二笔银行付款。
    SupplierDepositUsage(String),
}

impl PaymentAllocationSource {
    /// 创建预存款使用来源。
    ///
    /// # 参数
    /// * `id` - 已核实预存款使用的稳定身份。
    /// # 返回
    /// 返回规范化的来源。
    /// # 错误
    /// 身份为空或超长时拒绝。
    pub fn deposit_usage(id: impl Into<String>) -> Result<Self> {
        Ok(Self::SupplierDepositUsage(identity(id.into())?))
    }

    /// 读取普通付款身份，预存款使用不提供银行付款身份。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 普通付款返回原付款身份。
    /// # 错误
    /// 不返回错误。
    pub fn supplier_payment_id(&self) -> Option<&SupplierPaymentId> {
        match self {
            Self::SupplierPayment(id) => Some(id),
            Self::SupplierDepositUsage(_) => None,
        }
    }

    /// 读取预存款使用身份。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 预存款使用返回原使用身份。
    /// # 错误
    /// 不返回错误。
    pub fn deposit_usage_id(&self) -> Option<&str> {
        match self {
            Self::SupplierPayment(_) => None,
            Self::SupplierDepositUsage(id) => Some(id),
        }
    }

    /// 校验直接构造枚举时也未绕过身份约束。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 身份有效时成功。
    /// # 错误
    /// 空白、未规范化或超长身份时拒绝。
    pub fn validate(&self) -> Result<()> {
        let raw = match self {
            Self::SupplierPayment(id) => id.as_ref(),
            Self::SupplierDepositUsage(id) => id,
        };
        if identity(raw.to_owned())? != raw {
            return Err(Error::from("核销资金来源身份须先规范化"));
        }
        Ok(())
    }
}

impl From<SupplierPaymentId> for PaymentAllocationSource {
    fn from(id: SupplierPaymentId) -> Self {
        Self::SupplierPayment(id)
    }
}

impl Serialize for PaymentAllocationSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::SupplierPayment(id) => map.serialize_entry("supplier_payment_id", id)?,
            Self::SupplierDepositUsage(id) => map.serialize_entry("supplier_deposit_usage_id", id)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for PaymentAllocationSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Identities {
            supplier_payment_id: Option<String>,
            supplier_deposit_usage_id: Option<String>,
        }
        let ids = Identities::deserialize(deserializer)?;
        let source = match (ids.supplier_payment_id, ids.supplier_deposit_usage_id) {
            (Some(id), None) => Self::SupplierPayment(SupplierPaymentId::new(id)),
            (None, Some(id)) => Self::SupplierDepositUsage(id),
            _ => return Err(D::Error::custom("核销必须且只能关联一个真实资金来源")),
        };
        source.validate().map_err(D::Error::custom)?;
        Ok(source)
    }
}

fn identity(value: String) -> Result<String> {
    normalize_required_text(value, "核销资金来源身份不能为空", 128, "核销资金来源身份过长")
}

#[cfg(test)]
mod tests {
    use serde_json::{from_value, json, to_value};

    use super::*;

    #[test]
    fn legacy_payment_and_deposit_keep_distinct_wire_identities() {
        let bank = PaymentAllocationSource::SupplierPayment(SupplierPaymentId::new("payment"));
        let deposit = PaymentAllocationSource::deposit_usage("usage").unwrap();
        for (source, wire) in [
            (bank, json!({"supplier_payment_id": "payment"})),
            (deposit, json!({"supplier_deposit_usage_id": "usage"})),
        ] {
            assert_eq!(to_value(&source).unwrap(), wire);
            assert_eq!(from_value::<PaymentAllocationSource>(wire).unwrap(), source);
        }
    }

    #[test]
    fn absent_ambiguous_or_invalid_sources_fail_closed() {
        for value in [
            json!({}),
            json!({"supplier_payment_id": "payment", "supplier_deposit_usage_id": "usage"}),
            json!({"supplier_payment_id": "  "}),
            json!({"supplier_deposit_usage_id": " usage "}),
        ] {
            assert!(from_value::<PaymentAllocationSource>(value).is_err());
        }
        assert!(PaymentAllocationSource::deposit_usage(" ").is_err());
    }
}
