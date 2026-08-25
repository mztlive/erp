//! `party_bank_account`：银行账户（数据模型 §6.2，P1 §2.1 敏感字段）。
//!
//! 账号是低熵敏感值（§4.5.5）：实体不保存明文，只保存
//! `account_number_ciphertext`（P3 加密填充）与 `account_number_query_hmac`
//! （带密钥 HMAC 查询指纹）；明文字段只出现在创建入参
//! [`PartyBankAccountData`] 中。查询与重复校验只能使用 keyed HMAC，
//! 不得回退为明文或裸哈希（§6.2）。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::sensitive::{hmac_sha256_hex, normalize_account_number};
use super::status::EffectiveRecordStatus;

pub use crate::ids::{PartyBankAccountId, PartyId};

/// 账户编号最大长度。
const BANK_ACCOUNT_NO_MAX_LEN: usize = 64;
/// 户名最大长度。
const ACCOUNT_NAME_MAX_LEN: usize = 200;
/// 银行名称最大长度。
const BANK_NAME_MAX_LEN: usize = 128;
/// 支行名称最大长度。
const BANK_BRANCH_NAME_MAX_LEN: usize = 128;
/// 账号明文最大长度。
const ACCOUNT_NUMBER_MAX_LEN: usize = 64;

/// 银行账户创建数据（不含系统字段）。
///
/// `account_number` 为账号明文，仅用于指纹计算与后续加密，实体不保留明文。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyBankAccountData {
    /// ERP 内部稳定账户编号（全局唯一，创建后不可修改）。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 户名。
    pub account_name: String,
    /// 银行名称。
    pub bank_name: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 账号（明文入参；敏感值，§4.5.5）。
    pub account_number: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
}

/// 银行账户更新数据。
///
/// 账户内容按有效期事实追加/结束维护（W03：新增与修改仅财务），
/// 原地更新只允许切换启停状态、结束有效期与调整默认标记。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyBankAccountUpdate {
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期更新意图（`Set` 时校验晚于 `valid_from`）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub valid_to: FieldUpdate<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

/// 银行账户实体（§6.2；账号密文 + 带密钥 HMAC 查询指纹，P1 §2.1）。
///
/// 自定义 `Debug`：账号密文与指纹字段不进入任何输出，明文永不入库。
#[derive(Serialize, Deserialize, Clone, Entity)]
pub struct PartyBankAccount {
    #[serde(flatten)]
    pub base: BaseModel,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 户名。
    pub account_name: String,
    /// 银行名称。
    pub bank_name: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 账号密文（数据库加密列；P3 按密钥体系填充，P1 定义字段与校验）。
    pub account_number_ciphertext: String,
    /// 规范化账号的带密钥 HMAC 查询指纹（低熵值精确查询，禁止裸摘要）。
    pub account_number_query_hmac: String,
    /// 账号末四位，仅用于生成不可逆的列表掩码；历史记录缺省为空。
    #[serde(default)]
    pub account_number_last4: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 是否为当前默认账户（同一主体同一时点最多一个默认有效账户，
    /// 跨行约束由 P3 事务校验，§6.2）。
    pub is_default: bool,
    /// 创建人。
    pub created_by: String,
    /// 最后更新人。
    pub updated_by: String,
}

impl fmt::Debug for PartyBankAccount {
    /// Redacted Debug：不输出账号密文与指纹（明文字段永不进入 Debug 输出）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PartyBankAccount")
            .field("base", &self.base)
            .field("bank_account_no", &self.bank_account_no)
            .field("party_id", &self.party_id)
            .field("account_name", &self.account_name)
            .field("bank_name", &self.bank_name)
            .field("bank_branch_name", &self.bank_branch_name)
            .field("account_number", &"[REDACTED]")
            .field("valid_from", &self.valid_from)
            .field("valid_to", &self.valid_to)
            .field("status", &self.status)
            .field("is_default", &self.is_default)
            .field("created_by", &self.created_by)
            .field("updated_by", &self.updated_by)
            .finish()
    }
}

impl fmt::Debug for PartyBankAccountData {
    /// Redacted Debug：账号明文不进入任何输出。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PartyBankAccountData")
            .field("bank_account_no", &self.bank_account_no)
            .field("party_id", &self.party_id)
            .field("account_name", &self.account_name)
            .field("bank_name", &self.bank_name)
            .field("bank_branch_name", &self.bank_branch_name)
            .field("account_number", &"[REDACTED]")
            .field("valid_from", &self.valid_from)
            .field("valid_to", &self.valid_to)
            .field("status", &self.status)
            .field("is_default", &self.is_default)
            .finish()
    }
}

impl PartyBankAccount {
    /// 生成账号查询指纹。
    ///
    /// 规范化规则：去首尾空白并移除空格、连字符与下划线后，以带密钥
    /// HMAC-SHA256 计算并输出 64 位小写 hex（§6.2：查询和重复校验只能
    /// 使用 keyed HMAC；换密钥后旧指纹全部失效）。
    ///
    /// # 参数
    /// * `plain` - 账号明文
    /// * `key` - 查询密钥
    ///
    /// # 返回
    /// 返回指纹字符串。
    pub fn account_number_fingerprint(plain: &str, key: &[u8]) -> String {
        hmac_sha256_hex(key, normalize_account_number(plain).as_bytes())
    }

    /// 创建银行账户。
    ///
    /// 完成编号、户名、银行名称的必填校验与文本规范化，账号明文
    /// 校验非空与长度；以 `fingerprint_key` 从明文账号生成查询指纹，
    /// 密文字段留空由 P3 加密填充；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PartyBankAccountId`）
    /// * `data` - 创建数据（含账号明文）
    /// * `fingerprint_key` - 查询指纹密钥
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的银行账户实体。
    ///
    /// # 错误
    /// 当必填字段为空/超长、账号为空/超长或生效区间倒挂时返回错误。
    pub fn new(
        id: PartyBankAccountId,
        data: PartyBankAccountData,
        fingerprint_key: &[u8],
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let bank_account_no = normalize_required_text(
            data.bank_account_no,
            "账户编号不能为空",
            BANK_ACCOUNT_NO_MAX_LEN,
            "账户编号过长",
        )?;
        let account_name = normalize_required_text(
            data.account_name,
            "户名不能为空",
            ACCOUNT_NAME_MAX_LEN,
            "户名过长",
        )?;
        let bank_name = normalize_required_text(
            data.bank_name,
            "银行名称不能为空",
            BANK_NAME_MAX_LEN,
            "银行名称过长",
        )?;
        let bank_branch_name =
            normalize_optional_text(data.bank_branch_name, "支行名称", BANK_BRANCH_NAME_MAX_LEN)?;
        let account_number = normalize_required_text(
            data.account_number,
            "账号不能为空",
            ACCOUNT_NUMBER_MAX_LEN,
            "账号过长",
        )?;
        let normalized_account_number = normalize_account_number(&account_number);
        let account_number_last4 = normalized_account_number
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let created_by = created_by.into();
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            bank_account_no,
            party_id: data.party_id,
            account_name,
            bank_name,
            bank_branch_name,
            account_number_ciphertext: String::new(),
            account_number_query_hmac: Self::account_number_fingerprint(&account_number, fingerprint_key),
            account_number_last4,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            status: data.status,
            is_default: data.is_default,
            updated_by: created_by.clone(),
            created_by,
        })
    }

    /// 更新银行账户（仅限生命周期字段）。
    ///
    /// 账户内容变更必须通过新的有效期事实行追加/结束（W03）；原地
    /// 更新只允许切换启停状态（固定状态机）、结束有效期与调整默认标记。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态迁移非法或 `valid_to` 不晚于 `valid_from` 时返回错误。
    pub fn update(&mut self, update: PartyBankAccountUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(to) = update.status {
            self.status.transition_to(to)?;
        }
        if let Some(valid_to) = update.valid_to.into_option() {
            ensure_window_valid(self.valid_from, Some(valid_to))?;
            self.valid_to = Some(valid_to);
        }
        if let Some(is_default) = update.is_default {
            self.is_default = is_default;
        }
        self.updated_by = updated_by.into();
        Ok(())
    }

    /// 判断账户是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("生效结束日期必须晚于生效开始日期"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PartyBankAccount, PartyBankAccountData, PartyBankAccountUpdate};
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{PartyBankAccountId, PartyId};
    use crate::party::status::EffectiveRecordStatus;

    const KEY: &[u8] = b"test-fingerprint-key";

    fn bank_account_data() -> PartyBankAccountData {
        PartyBankAccountData {
            bank_account_no: " BA-2026-001 ".to_string(),
            party_id: PartyId::new("party-1"),
            account_name: " 上海示例科技有限公司 ".to_string(),
            bank_name: " 招商银行 ".to_string(),
            bank_branch_name: Some(" 上海分行 ".to_string()),
            account_number: " 6225-8802_1234 5678 ".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            is_default: true,
            status: EffectiveRecordStatus::Active,
        }
    }

    /// happy path：编号/户名去空白，账号移除分隔符后生成指纹，实体不含明文。
    #[test]
    fn new_normalizes_and_fingerprints() {
        let account = PartyBankAccount::new(
            PartyBankAccountId::new("ba-1"),
            bank_account_data(),
            KEY,
            "admin-1",
        )
        .unwrap();
        assert_eq!(account.bank_account_no, "BA-2026-001");
        assert_eq!(account.account_name, "上海示例科技有限公司");
        assert_eq!(account.bank_name, "招商银行");
        assert_eq!(account.bank_branch_name.as_deref(), Some("上海分行"));
        assert_eq!(
            account.account_number_query_hmac,
            PartyBankAccount::account_number_fingerprint("6225-8802_1234 5678", KEY)
        );
        assert_eq!(
            account.account_number_query_hmac,
            PartyBankAccount::account_number_fingerprint("6225880212345678", KEY),
            "分隔符不参与指纹"
        );
        assert!(account.account_number_ciphertext.is_empty(), "P3 加密填充");
        assert!(account.is_active());
        assert!(account.is_default);
    }

    /// 失败路径：必填为空/超长、账号为空/超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = PartyBankAccountData {
            bank_account_no: "   ".to_string(),
            ..bank_account_data()
        };
        assert!(PartyBankAccount::new(PartyBankAccountId::new("b"), blank_no, KEY, "admin-1").is_err());

        let blank_number = PartyBankAccountData {
            account_number: "   ".to_string(),
            ..bank_account_data()
        };
        assert!(PartyBankAccount::new(PartyBankAccountId::new("b"), blank_number, KEY, "admin-1").is_err());

        let overlong_name = PartyBankAccountData {
            account_name: "x".repeat(201),
            ..bank_account_data()
        };
        assert!(PartyBankAccount::new(PartyBankAccountId::new("b"), overlong_name, KEY, "admin-1").is_err());

        let reversed = PartyBankAccountData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..bank_account_data()
        };
        assert!(PartyBankAccount::new(PartyBankAccountId::new("b"), reversed, KEY, "admin-1").is_err());
    }

    /// 状态机：启停切换经固定矩阵校验，非法目标被拒。
    #[test]
    fn status_transitions_are_validated() {
        let mut account = PartyBankAccount::new(
            PartyBankAccountId::new("ba-2"),
            bank_account_data(),
            KEY,
            "admin-1",
        )
        .unwrap();
        account
            .update(
                PartyBankAccountUpdate {
                    status: Some(EffectiveRecordStatus::Disabled),
                    valid_to: FieldUpdate::Unchanged,
                    is_default: None,
                },
                "admin-2",
            )
            .unwrap();
        assert!(!account.is_active());
        account
            .update(
                PartyBankAccountUpdate {
                    status: Some(EffectiveRecordStatus::Active),
                    valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 6, 30).unwrap()),
                    is_default: Some(false),
                },
                "admin-3",
            )
            .unwrap();
        assert!(account.is_active());
        assert_eq!(
            account.valid_to,
            Some(BusinessDate::from_ymd(2026, 6, 30).unwrap())
        );
        assert!(!account.is_default);
        assert_eq!(account.updated_by, "admin-3");
    }

    /// 敏感字段：Debug 与 Debug(Data) 均不泄漏账号明文与指纹。
    #[test]
    fn debug_never_leaks_plaintext() {
        let data = bank_account_data();
        let account =
            PartyBankAccount::new(PartyBankAccountId::new("ba-3"), data.clone(), KEY, "admin-1").unwrap();

        let debug_entity = format!("{account:?}");
        assert!(!debug_entity.contains("6225"));
        assert!(!debug_entity.contains("account_number_query_hmac"));
        assert!(!debug_entity.contains("account_number_ciphertext"));
        assert!(debug_entity.contains("[REDACTED]"));

        let debug_data = format!("{data:?}");
        assert!(!debug_data.contains("6225"));
        assert!(debug_data.contains("[REDACTED]"));
    }

    /// 敏感字段：指纹稳定（同密钥同明文）且带密钥（换密钥指纹不同）。
    #[test]
    fn fingerprint_is_stable_and_keyed() {
        let a = PartyBankAccount::account_number_fingerprint("6225880212345678", b"key-a");
        let b = PartyBankAccount::account_number_fingerprint("6225880212345678", b"key-a");
        let c = PartyBankAccount::account_number_fingerprint("6225880212345678", b"key-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
        assert_ne!(a, "6225880212345678", "指纹不得等于明文");
    }

    /// 实体 BSON 往返（密文与指纹保持原样，不出现明文）。
    #[test]
    fn bson_roundtrip() {
        let account = PartyBankAccount::new(
            PartyBankAccountId::new("ba-4"),
            bank_account_data(),
            KEY,
            "admin-1",
        )
        .unwrap();
        let roundtrip: PartyBankAccount =
            bson::deserialize_from_document(bson::serialize_to_document(&account).unwrap()).unwrap();
        assert_eq!(roundtrip.base, account.base);
        assert_eq!(
            roundtrip.account_number_query_hmac,
            account.account_number_query_hmac
        );
    }
}
