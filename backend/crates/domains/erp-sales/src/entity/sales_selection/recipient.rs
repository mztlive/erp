//! 提货券提交时冻结的个人收件信息。

use std::fmt;

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 个人收件信息，不包含未提交选择或其它参与人的数据。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionRecipient {
    /// 收件人姓名。
    pub name: String,
    /// 联系电话。
    pub phone: String,
    /// 省。
    pub province: String,
    /// 市。
    pub city: String,
    /// 区县。
    pub district: String,
    /// 详细地址。
    pub address: String,
}

impl fmt::Debug for SelectionRecipient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SelectionRecipient { [REDACTED] }")
    }
}

impl SelectionRecipient {
    /// 构造并规范化完整收件信息。
    ///
    /// # 参数
    /// * `data` - 请求提供的姓名、电话及地址
    ///
    /// # 返回
    /// 返回去除字段首尾空白的收件信息。
    ///
    /// # 错误
    /// 任一字段为空、超长，或电话不是 7 到 20 位数字及可选前导加号时拒绝。
    pub fn new(data: Self) -> Result<Self> {
        let phone = normalize_required_text(data.phone, "收件电话不能为空", 21, "收件电话过长")?;
        let digits = phone.strip_prefix('+').unwrap_or(&phone);
        if !(7..=20).contains(&digits.len()) || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::from("收件电话必须是 7 到 20 位数字，可带前导加号"));
        }
        Ok(Self {
            name: normalize_required_text(data.name, "收件人不能为空", 64, "收件人姓名过长")?,
            phone,
            province: normalize_required_text(data.province, "省不能为空", 64, "省名称过长")?,
            city: normalize_required_text(data.city, "市不能为空", 64, "市名称过长")?,
            district: normalize_required_text(data.district, "区县不能为空", 64, "区县名称过长")?,
            address: normalize_required_text(data.address, "详细地址不能为空", 256, "详细地址过长")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipient() -> SelectionRecipient {
        SelectionRecipient {
            name: " 张三 ".into(),
            phone: " 13800138000 ".into(),
            province: " 浙江省 ".into(),
            city: " 杭州市 ".into(),
            district: " 西湖区 ".into(),
            address: " 文三路 1 号 ".into(),
        }
    }

    #[test]
    fn complete_address_is_normalized_and_debug_is_redacted() {
        let value = SelectionRecipient::new(recipient()).unwrap();
        assert_eq!(value.name, "张三");
        assert_eq!(value.phone, "13800138000");
        assert_eq!(value.address, "文三路 1 号");
        assert!(!format!("{value:?}").contains("13800138000"));
    }

    #[test]
    fn missing_address_and_invalid_phone_are_rejected() {
        let mut value = recipient();
        value.address = "  ".into();
        assert!(SelectionRecipient::new(value).is_err());
        let mut value = recipient();
        value.phone = "138ABC13800".into();
        assert!(SelectionRecipient::new(value).is_err());
        let mut value = recipient();
        value.name = "中".repeat(65);
        assert!(SelectionRecipient::new(value).is_err());
    }
}
