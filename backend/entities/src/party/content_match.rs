//! 从属事实内容等价：只比较预计算 fingerprint 与非敏感规范值。
//!
//! 密钥、HMAC、加密、解密和敏感明文不进入本模块。敏感明文缺失或空白时
//! 沿用原事实；银行账户稳定内容变化由 [`PartyBankAccount::ensure_unmodified`]
//! 拒绝原地修改。

use super::party_address::{AddressType, PartyAddress};
use super::party_bank_account::PartyBankAccount;
use super::party_contact::PartyContact;
use crate::errors::{Error, Result};

/// 调用方预计算的查询指纹（HMAC 十六进制结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFingerprint(String);

impl QueryFingerprint {
    /// 包装已经由 Service/crypto port 算好的查询指纹。
    ///
    /// 本方法不持有密钥、不计算 HMAC、不接受敏感明文。
    ///
    /// # 参数
    /// * `hex` - 预计算的指纹字符串
    ///
    /// # 返回
    /// 返回可供实体比较的强类型指纹。
    ///
    /// # 错误
    /// 无。指纹格式由计算方保证。
    ///
    /// # 关键业务约束
    /// 不得在此计算 HMAC 或接收密钥。
    pub fn from_precomputed(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    /// 返回指纹字符串。
    ///
    /// # 返回
    /// 返回预计算指纹的借用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 指纹不是明文，但仍不得写入日志以外的敏感上下文。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 敏感字段相对既有事实的比较方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensitiveFactReuse {
    /// 请求未携带敏感明文或明文为空白，沿用原事实。
    ReuseOriginal,
    /// 使用预计算 fingerprint 与库存指纹比较。
    Fingerprint(QueryFingerprint),
}

impl SensitiveFactReuse {
    /// 返回沿用原事实的比较意图。
    ///
    /// Service 在敏感明文缺失或去空白后为空时使用本构造；本方法不接收明文、
    /// 密钥或指纹计算闭包。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回沿用原事实的比较意图。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 沿用原事实表示跳过敏感字段比较，仅比较非敏感规范值。
    pub fn reuse_original() -> Self {
        Self::ReuseOriginal
    }

    /// 由 Service 预计算指纹构造敏感字段比较意图。
    ///
    /// 指纹必须由 Service/crypto port 持有密钥算好后传入；本方法不接收明文、
    /// 密钥或计算闭包。
    ///
    /// # 参数
    /// * `fingerprint` - Service 预计算的强类型查询指纹
    ///
    /// # 返回
    /// 返回携带预计算指纹的比较意图。
    ///
    /// # 错误
    /// 无。指纹格式由计算方保证。
    ///
    /// # 关键业务约束
    /// 密钥与敏感明文不得进入本 VO。
    pub fn from_fingerprint(fingerprint: QueryFingerprint) -> Self {
        Self::Fingerprint(fingerprint)
    }

    /// 判断敏感字段是否与库存指纹一致。
    ///
    /// # 参数
    /// * `stored_hmac` - 实体已持久化的查询指纹
    ///
    /// # 返回
    /// 沿用原事实或指纹相等时返回 `true`。
    fn matches(&self, stored_hmac: &str) -> bool {
        match self {
            Self::ReuseOriginal => true,
            Self::Fingerprint(fingerprint) => fingerprint.as_str() == stored_hmac,
        }
    }
}

/// 联系人非敏感规范值与手机号比较输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyContactContentMatch {
    contact_name: String,
    title: Option<String>,
    telephone: Option<String>,
    email: Option<String>,
    mobile: SensitiveFactReuse,
}

impl PartyContactContentMatch {
    /// 构造联系人内容比较输入并规范化可选文本。
    ///
    /// # 参数
    /// * `contact_name` - 联系人姓名；比较前去除首尾空白
    /// * `title` - 职务；空白视为未提供
    /// * `telephone` - 固话；空白视为未提供
    /// * `email` - 邮箱；空白视为未提供
    /// * `mobile` - 手机号比较意图
    ///
    /// # 返回
    /// 返回可与既有联系人比较的值对象。
    ///
    /// # 错误
    /// 无。空姓名只导致比较失败，不在本构造拒绝。
    ///
    /// # 关键业务约束
    /// 不接收密钥或敏感明文；手机号比较必须使用预计算 fingerprint。
    pub fn new(
        contact_name: impl AsRef<str>,
        title: Option<String>,
        telephone: Option<String>,
        email: Option<String>,
        mobile: SensitiveFactReuse,
    ) -> Self {
        Self {
            contact_name: contact_name.as_ref().trim().to_string(),
            title: canonicalize_optional(title),
            telephone: canonicalize_optional(telephone),
            email: canonicalize_optional(email),
            mobile,
        }
    }
}

/// 地址非敏感规范值与地址内容比较输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyAddressContentMatch {
    address_type: AddressType,
    contact_name: Option<String>,
    address: SensitiveFactReuse,
}

impl PartyAddressContentMatch {
    /// 构造地址内容比较输入并规范化可选文本。
    ///
    /// # 参数
    /// * `address_type` - 地址类型
    /// * `contact_name` - 地址联系人；空白视为未提供
    /// * `address` - 地址明文比较意图
    ///
    /// # 返回
    /// 返回可与既有地址比较的值对象。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不接收密钥或敏感明文。
    pub fn new(address_type: AddressType, contact_name: Option<String>, address: SensitiveFactReuse) -> Self {
        Self {
            address_type,
            contact_name: canonicalize_optional(contact_name),
            address,
        }
    }
}

/// 银行账户非敏感规范值与账号比较输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyBankAccountContentMatch {
    account_name: String,
    bank_name: String,
    bank_branch_name: Option<String>,
    account_number: SensitiveFactReuse,
}

impl PartyBankAccountContentMatch {
    /// 构造银行账户内容比较输入并规范化可选文本。
    ///
    /// # 参数
    /// * `account_name` - 户名；比较前去除首尾空白
    /// * `bank_name` - 银行名称；比较前去除首尾空白
    /// * `bank_branch_name` - 支行名称；空白视为未提供
    /// * `account_number` - 账号比较意图
    ///
    /// # 返回
    /// 返回可与既有银行账户比较的值对象。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不接收密钥或敏感明文。
    pub fn new(
        account_name: impl AsRef<str>,
        bank_name: impl AsRef<str>,
        bank_branch_name: Option<String>,
        account_number: SensitiveFactReuse,
    ) -> Self {
        Self {
            account_name: account_name.as_ref().trim().to_string(),
            bank_name: bank_name.as_ref().trim().to_string(),
            bank_branch_name: canonicalize_optional(bank_branch_name),
            account_number,
        }
    }
}

impl PartyContact {
    /// 判断既有联系人是否与比较输入内容等价。
    ///
    /// 敏感明文缺失或空白时沿用原手机号事实；其余字段按去空白后的规范值比较。
    ///
    /// # 参数
    /// * `candidate` - 预计算指纹与非敏感规范值
    ///
    /// # 返回
    /// 内容一致时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 敏感明文缺失或空白时沿用原手机号事实。
    pub fn matches_content(&self, candidate: &PartyContactContentMatch) -> bool {
        candidate.mobile.matches(&self.mobile_query_hmac)
            && self.contact_name == candidate.contact_name
            && optional_eq(&self.title, &candidate.title)
            && optional_eq(&self.telephone, &candidate.telephone)
            && optional_eq(&self.email, &candidate.email)
    }
}

impl PartyAddress {
    /// 判断既有地址是否与比较输入内容等价。
    ///
    /// 敏感明文缺失或空白时沿用原地址事实。
    ///
    /// # 参数
    /// * `candidate` - 预计算指纹与非敏感规范值
    ///
    /// # 返回
    /// 内容一致时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 敏感明文缺失或空白时沿用原地址事实。
    pub fn matches_content(&self, candidate: &PartyAddressContentMatch) -> bool {
        candidate.address.matches(&self.address_query_hmac)
            && self.address_type == candidate.address_type
            && optional_eq(&self.contact_name, &candidate.contact_name)
    }
}

impl PartyBankAccount {
    /// 判断既有银行账户稳定内容是否未被修改。
    ///
    /// 敏感明文缺失或空白时沿用原账号事实。
    ///
    /// # 参数
    /// * `candidate` - 预计算指纹与非敏感规范值
    ///
    /// # 返回
    /// 稳定内容一致时返回 `true`。
    ///
    /// # 错误
    /// 无。原地修改拒绝见 [`Self::ensure_unmodified`]。
    ///
    /// # 关键业务约束
    /// 敏感明文缺失或空白时沿用原账号事实。
    pub fn matches_content(&self, candidate: &PartyBankAccountContentMatch) -> bool {
        candidate.account_number.matches(&self.account_number_query_hmac)
            && self.account_name == candidate.account_name
            && self.bank_name == candidate.bank_name
            && optional_eq(&self.bank_branch_name, &candidate.bank_branch_name)
    }

    /// 拒绝既有银行账户稳定内容被原地修改。
    ///
    /// # 参数
    /// * `candidate` - 预计算指纹与非敏感规范值
    ///
    /// # 返回
    /// 内容未变化时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一稳定字段变化时返回 [`Error::LogicError`]，提示结束旧账户后新增。
    ///
    /// # 关键业务约束
    /// 银行账户内容只能追加/结束，不能原地改写户名、银行、支行或账号。
    pub fn ensure_unmodified(&self, candidate: &PartyBankAccountContentMatch) -> Result<()> {
        if self.matches_content(candidate) {
            return Ok(());
        }
        Err(Error::from(
            "既有银行账户内容不可原地修改，请结束旧账户后新增账户",
        ))
    }
}

/// 将可选文本规范为去空白后的非空值。
fn canonicalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// 比较库存可选文本与已规范化期望值。
fn optional_eq(stored: &Option<String>, expected: &Option<String>) -> bool {
    stored.as_deref().map(str::trim).filter(|value| !value.is_empty()) == expected.as_deref()
}

#[cfg(test)]
mod tests {
    use super::{
        PartyAddressContentMatch, PartyBankAccountContentMatch, PartyContactContentMatch, QueryFingerprint,
        SensitiveFactReuse,
    };
    use crate::common::time::BusinessDate;
    use crate::ids::{PartyAddressId, PartyBankAccountId, PartyContactId, PartyId};
    use crate::party::party_address::{AddressType, PartyAddress, PartyAddressData};
    use crate::party::party_bank_account::{PartyBankAccount, PartyBankAccountData};
    use crate::party::party_contact::{PartyContact, PartyContactData};
    use crate::party::status::EffectiveRecordStatus;

    const KEY: &[u8] = b"content-match-test-key";

    fn contact() -> PartyContact {
        PartyContact::new(
            PartyContactId::new("contact-1"),
            PartyContactData {
                party_id: PartyId::new("party-1"),
                contact_name: "张三".to_string(),
                title: Some("采购负责人".to_string()),
                mobile: "13800138000".to_string(),
                telephone: Some("021-12345678".to_string()),
                email: Some("zhangsan@example.com".to_string()),
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap()
    }

    fn address() -> PartyAddress {
        PartyAddress::new(
            PartyAddressId::new("addr-1"),
            PartyAddressData {
                party_id: PartyId::new("party-1"),
                address_type: AddressType::Fulfillment,
                contact_name: Some("张三".to_string()),
                address: "北京市朝阳区望京街10号".to_string(),
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap()
    }

    fn bank_account() -> PartyBankAccount {
        PartyBankAccount::new(
            PartyBankAccountId::new("ba-1"),
            PartyBankAccountData {
                bank_account_no: "BA-2026-001".to_string(),
                party_id: PartyId::new("party-1"),
                account_name: "上海示例科技有限公司".to_string(),
                bank_name: "招商银行".to_string(),
                bank_branch_name: Some("上海分行".to_string()),
                account_number: "6225880212345678".to_string(),
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap()
    }

    fn mobile_fp(plain: &str) -> QueryFingerprint {
        QueryFingerprint::from_precomputed(PartyContact::mobile_fingerprint(plain, KEY))
    }

    fn address_fp(plain: &str) -> QueryFingerprint {
        QueryFingerprint::from_precomputed(PartyAddress::address_fingerprint(plain, KEY))
    }

    fn account_fp(plain: &str) -> QueryFingerprint {
        QueryFingerprint::from_precomputed(PartyBankAccount::account_number_fingerprint(plain, KEY))
    }

    fn contact_match(mobile: SensitiveFactReuse) -> PartyContactContentMatch {
        PartyContactContentMatch::new(
            " 张三 ",
            Some(" 采购负责人 ".to_string()),
            Some(" 021-12345678 ".to_string()),
            Some(" zhangsan@example.com ".to_string()),
            mobile,
        )
    }

    #[test]
    fn contact_reuses_original_when_sensitive_plain_missing_or_blank() {
        let contact = contact();
        let missing = contact_match(SensitiveFactReuse::reuse_original());
        let blank = contact_match(SensitiveFactReuse::reuse_original());
        assert!(contact.matches_content(&missing));
        assert!(contact.matches_content(&blank));
    }

    #[test]
    fn contact_matches_same_fingerprint_and_rejects_changed_sensitive_or_fields() {
        let contact = contact();
        let same = contact_match(SensitiveFactReuse::from_fingerprint(mobile_fp(" 13800138000 ")));
        assert!(contact.matches_content(&same));

        let changed_mobile = contact_match(SensitiveFactReuse::from_fingerprint(mobile_fp("13900139000")));
        assert!(!contact.matches_content(&changed_mobile));

        let changed_name = PartyContactContentMatch::new(
            "李四",
            Some("采购负责人".to_string()),
            Some("021-12345678".to_string()),
            Some("zhangsan@example.com".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!contact.matches_content(&changed_name));

        let changed_title = PartyContactContentMatch::new(
            "张三",
            Some("普通员工".to_string()),
            Some("021-12345678".to_string()),
            Some("zhangsan@example.com".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!contact.matches_content(&changed_title));

        let changed_telephone = PartyContactContentMatch::new(
            "张三",
            Some("采购负责人".to_string()),
            Some("021-87654321".to_string()),
            Some("zhangsan@example.com".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!contact.matches_content(&changed_telephone));

        let changed_email = PartyContactContentMatch::new(
            "张三",
            Some("采购负责人".to_string()),
            Some("021-12345678".to_string()),
            Some("other@example.com".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!contact.matches_content(&changed_email));

        let empty_title_equals_none = PartyContactContentMatch::new(
            "张三",
            Some("   ".to_string()),
            Some("021-12345678".to_string()),
            Some("zhangsan@example.com".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!contact.matches_content(&empty_title_equals_none));

        let untitled = PartyContact::new(
            PartyContactId::new("contact-2"),
            PartyContactData {
                party_id: PartyId::new("party-1"),
                contact_name: "张三".to_string(),
                title: None,
                mobile: "13800138000".to_string(),
                telephone: None,
                email: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: false,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap();
        let blank_optionals = PartyContactContentMatch::new(
            "张三",
            Some("  ".to_string()),
            Some("".to_string()),
            None,
            SensitiveFactReuse::reuse_original(),
        );
        assert!(untitled.matches_content(&blank_optionals));
    }

    #[test]
    fn address_matches_canonical_optional_text_and_fingerprint() {
        let address = address();
        let same = PartyAddressContentMatch::new(
            AddressType::Fulfillment,
            Some(" 张三 ".to_string()),
            SensitiveFactReuse::from_fingerprint(address_fp("北京市朝阳区望京街10号")),
        );
        assert!(address.matches_content(&same));

        let missing = PartyAddressContentMatch::new(
            AddressType::Fulfillment,
            Some("张三".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(address.matches_content(&missing));

        let blank = PartyAddressContentMatch::new(
            AddressType::Fulfillment,
            Some("张三".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(address.matches_content(&blank));

        let changed_address = PartyAddressContentMatch::new(
            AddressType::Fulfillment,
            Some("张三".to_string()),
            SensitiveFactReuse::from_fingerprint(address_fp("北京市朝阳区望京街11号")),
        );
        assert!(!address.matches_content(&changed_address));

        let changed_contact_name = PartyAddressContentMatch::new(
            AddressType::Fulfillment,
            Some("李四".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!address.matches_content(&changed_contact_name));

        let type_changed = PartyAddressContentMatch::new(
            AddressType::Registered,
            Some("张三".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(!address.matches_content(&type_changed));
    }

    #[test]
    fn bank_account_forbids_in_place_stable_content_change() {
        let account = bank_account();
        let same = PartyBankAccountContentMatch::new(
            " 上海示例科技有限公司 ",
            " 招商银行 ",
            Some(" 上海分行 ".to_string()),
            SensitiveFactReuse::from_fingerprint(account_fp("6225-8802-1234-5678")),
        );
        assert!(account.matches_content(&same));
        assert!(account.ensure_unmodified(&same).is_ok());

        let missing_number = PartyBankAccountContentMatch::new(
            "上海示例科技有限公司",
            "招商银行",
            Some("上海分行".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(account.ensure_unmodified(&missing_number).is_ok());

        let blank_number = PartyBankAccountContentMatch::new(
            "上海示例科技有限公司",
            "招商银行",
            Some("上海分行".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(account.ensure_unmodified(&blank_number).is_ok());

        let changed_name = PartyBankAccountContentMatch::new(
            "其他公司",
            "招商银行",
            Some("上海分行".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        let error = account.ensure_unmodified(&changed_name).unwrap_err();
        assert_eq!(
            error.to_string(),
            "既有银行账户内容不可原地修改，请结束旧账户后新增账户"
        );

        let changed_bank = PartyBankAccountContentMatch::new(
            "上海示例科技有限公司",
            "工商银行",
            Some("上海分行".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(account.ensure_unmodified(&changed_bank).is_err());

        let changed_branch = PartyBankAccountContentMatch::new(
            "上海示例科技有限公司",
            "招商银行",
            Some("北京分行".to_string()),
            SensitiveFactReuse::reuse_original(),
        );
        assert!(account.ensure_unmodified(&changed_branch).is_err());

        let changed_number = PartyBankAccountContentMatch::new(
            "上海示例科技有限公司",
            "招商银行",
            Some("上海分行".to_string()),
            SensitiveFactReuse::from_fingerprint(account_fp("6225880299999999")),
        );
        assert!(account.ensure_unmodified(&changed_number).is_err());
    }
}
