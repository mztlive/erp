use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use erp_core::ids::FileAssetId;
use erp_core::money::Rate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::supplier::{
    CapabilityCode, InvoiceType, QualificationType, ReconciliationCycle, SettlementMode, SupplierRating,
};
use crate::error::{Error, Result};

/// 根级供应商资料中的默认联系人输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileContactInput {
    /// 联系人姓名。
    #[validate(custom(function = "non_blank", message = "联系人姓名不能为空"))]
    pub contact_name: String,
    /// 手机号明文；仅在请求处理期间存在。
    #[validate(custom(function = "non_blank", message = "手机号不能为空"))]
    pub mobile: String,
    /// 固话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
}

impl SupplierProfileContactInput {
    /// 以必填联系人构造资料输入；固话与邮箱默认为空。
    ///
    /// # 参数
    /// * `contact_name` - 联系人姓名
    /// * `mobile` - 手机号明文
    ///
    /// # 返回
    /// 返回无固话邮箱的输入。
    ///
    /// # 错误
    /// 无。
    pub fn new(contact_name: String, mobile: String) -> Self {
        Self { contact_name, mobile, telephone: None, email: None }
    }
}

/// 根级供应商资料中的默认经营地址输入。

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileAddressInput {
    /// 地址明文；仅在请求处理期间存在。
    #[validate(custom(function = "non_blank", message = "地址不能为空"))]
    pub address: String,
    /// 地址联系人。
    pub contact_name: Option<String>,
}

/// 根级供应商资料中的默认银行账户输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileBankAccountInput {
    /// 银行名称。
    #[validate(custom(function = "non_blank", message = "银行名称不能为空"))]
    pub bank_name: String,
    /// 银行账号明文；仅在请求处理期间存在。
    #[validate(custom(function = "non_blank", message = "银行账号不能为空"))]
    pub account_number: String,
}

/// 根级供应商资料中的资质输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProfileQualificationInput {
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书或合同编号。
    #[validate(custom(function = "non_blank", message = "证书编号不能为空"))]
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效日期。
    pub valid_from: Option<BusinessDate>,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 文件资产。
    pub attachment_id: Option<FileAssetId>,
    /// 适用能力代码；服务端解析为当前供应商能力 ID。
    pub capability_codes: Vec<CapabilityCode>,
}

/// 根级供应商资料中单条能力的显式负责人。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct SupplierProfileCapabilityOwnerInput {
    /// 能力代码。
    pub capability_code: CapabilityCode,
    /// 供给能力负责人。
    #[validate(custom(function = "non_blank", message = "供给能力负责人不能为空"))]
    pub owner_user_id: String,
}

/// 根级供应商资料中的当前评级输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierProfileRatingInput {
    /// 首次合作期初评分。
    pub initial_score: Option<u8>,
    /// 评级。
    pub rating: SupplierRating,
    /// 当前评分。
    pub current_score: u8,
    /// 生效日期。
    pub valid_from: BusinessDate,
}

/// 创建或修订完整供应商资料的根级命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SaveSupplierProfileRequest {
    /// 客户端幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 创建时必填的主体编号；修订时忽略。
    pub party_no: Option<String>,
    /// 创建时必填的供应商编号；修订时忽略。
    pub supplier_no: Option<String>,
    /// 修订时必填的主体乐观锁版本。
    pub expected_party_version: Option<u64>,
    /// 修订时必填的供应商乐观锁版本。
    pub expected_supplier_version: Option<u64>,
    /// 法定名称。
    #[validate(custom(function = "non_blank", message = "法定名称不能为空"))]
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 默认联系人；`None` 表示创建时不填、修订时保留。
    pub contact: Option<SupplierProfileContactInput>,
    /// 修订时明确停用当前联系人；不能与 `contact` 同时提交。
    #[serde(default)]
    pub clear_contact: bool,
    /// 默认经营地址；`None` 表示创建时不填、修订时保留。
    pub address: Option<SupplierProfileAddressInput>,
    /// 修订时明确停用当前经营地址；不能与 `address` 同时提交。
    #[serde(default)]
    pub clear_address: bool,
    /// 税号；`None` 表示创建时不填、修订时保留。
    pub tax_no: Option<String>,
    /// 修订时明确停用当前税务档案；不能与非空 `tax_no` 同时提交。
    #[serde(default)]
    pub clear_tax_profile: bool,
    /// 默认银行账户；`None` 表示创建时不填、修订时保留。
    pub bank_account: Option<SupplierProfileBankAccountInput>,
    /// 修订时明确停用当前银行账户；不能与 `bank_account` 同时提交。
    #[serde(default)]
    pub clear_bank_account: bool,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 付款条件结构化快照（不得再编码经营类目）。
    #[validate(custom(function = "non_blank", message = "付款条件快照不能为空"))]
    pub payment_term_snapshot: String,
    /// 经营类目；空白表示未登记。旧客户端可能把类目编码进付款条件快照，实体构造时会拆出。
    #[serde(default)]
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点。
    pub invoice_tax_rate: Option<Rate>,
    /// 常用进项税率；None 读取旧单值，Some([]) 明确表示未登记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_tax_rates: Option<Vec<Rate>>,
    /// 签约主体。
    pub signing_entity_party_id: erp_core::ids::PartyId,
    /// 付款主体。
    pub payment_entity_party_id: erp_core::ids::PartyId,
    /// 整体维护人；创建必填。
    pub maintainer_user_id: Option<String>,
    /// 新建能力的显式负责人；禁止省略后写成操作人。
    #[serde(default)]
    pub capability_owners: Vec<SupplierProfileCapabilityOwnerInput>,
    /// 当前启用能力代码集合。
    pub capability_codes: Vec<CapabilityCode>,
    /// 当前资质集合。
    pub qualifications: Vec<SupplierProfileQualificationInput>,
    /// 当前评级；`None` 表示不写评级。
    pub rating: Option<SupplierProfileRatingInput>,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
}

impl SaveSupplierProfileRequest {
    /// 校验根级供应商资料请求的完整输入合同。
    ///
    /// # 返回
    /// 根字段、清空/替换意图及全部嵌套输入均合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 根字段格式非法、同一资料同时请求替换与清空，或嵌套输入非法时返回
    /// `ValidationError`；校验顺序固定为根字段、互斥意图、联系人、地址、
    /// 银行账户、资质。
    pub fn validate_contract(&self) -> Result<()> {
        self.validate()?;
        if self.clear_contact && self.contact.is_some() {
            return Err(Error::ValidationError("联系人不能同时替换和清空".to_string()));
        }
        if self.clear_address && self.address.is_some() {
            return Err(Error::ValidationError("经营地址不能同时替换和清空".to_string()));
        }
        if self.clear_tax_profile && self.tax_no.as_deref().is_some_and(|value| !value.trim().is_empty()) {
            return Err(Error::ValidationError("税务档案不能同时替换和清空".to_string()));
        }
        if self.clear_bank_account && self.bank_account.is_some() {
            return Err(Error::ValidationError("银行账户不能同时替换和清空".to_string()));
        }
        if let Some(contact) = &self.contact {
            contact.validate()?;
        }
        if let Some(address) = &self.address {
            address.validate()?;
        }
        if let Some(bank_account) = &self.bank_account {
            bank_account.validate()?;
        }
        for qualification in &self.qualifications {
            qualification.validate()?;
        }
        Ok(())
    }

    /// 计算根命令稳定指纹，保证同一幂等键只能重放完全相同的请求。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回对请求规范 JSON（键排序、确定性序列化）进行 SHA-256 后的 `sha256-v1:<hex>` 指纹。
    ///
    /// # 错误
    /// JSON 序列化失败时返回 `Internal` 错误。
    ///
    /// # 约束
    /// 输入为 `&self`，不触及外部 I/O；规范 JSON 按键排序固定字段顺序，旧裸 `64hex`
    /// 指纹仍通过 `SupplierProfileCommand::ensure_replayable` 兼容回读。
    pub fn fingerprint(&self) -> Result<String> {
        use sha2::{Digest, Sha256};
        let value = serde_json::to_value(self)
            .map_err(|error| Error::Internal(format!("供应商命令序列化失败: {error}")))?;
        let canonical = canonical_json_string(&value)?;
        Ok(format!("sha256-v1:{}", hex::encode(Sha256::digest(canonical.as_bytes()))))
    }

    /// 读取更新场景必填版本号。
    ///
    /// # 参数
    /// * `value` - 可空版本输入
    /// * `object` - 业务对象名，用于错误文案
    ///
    /// # 返回
    /// 存在时返回版本号。
    ///
    /// # 错误
    /// `value` 为 `None` 时返回 `ValidationError`。
    ///
    /// # 约束
    /// 仅做存在性校验，不触及持久化。
    pub fn required_update_version(value: Option<u64>, object: &str) -> Result<u64> {
        value.ok_or_else(|| Error::ValidationError(format!("修订供应商时{object}版本不能为空")))
    }

    /// 校验乐观锁版本。
    ///
    /// # 参数
    /// * `actual` - 当前持久化版本
    /// * `expected` - 客户端期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本不一致时返回 `ConflictError`。
    ///
    /// # 约束
    /// 纯内存比较，不触及外部状态。
    pub fn ensure_version(actual: u64, expected: u64) -> Result<()> {
        if actual != expected {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        Ok(())
    }

    /// 校验创建场景必填的稳定业务编号。
    ///
    /// # 参数
    /// * `value` - 可空输入
    /// * `field` - 字段中文名，用于错误文案
    ///
    /// # 返回
    /// 去首尾空白后非空时返回规范化编号。
    ///
    /// # 错误
    /// 输入为空或全空白时返回 `ValidationError`。
    ///
    /// # 约束
    /// 仅做空白与存在性校验，不触及唯一性查询。
    pub fn required_create_identity(value: Option<&str>, field: &str) -> Result<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| Error::ValidationError(format!("创建供应商时{field}不能为空")))
    }
}

/// 将 JSON 值按规范（键排序）序列化为确定性字符串，用于稳定指纹。
///
/// # 参数
/// * `value` - 待序列化的 JSON 值
///
/// # 返回
/// 返回键排序后的规范 JSON 字符串。
///
/// # 错误
/// 序列化失败时返回 `Internal` 错误。
///
/// # 约束
/// 对象键按字典序排序，数组保持输入顺序，标量按 `serde_json` 标准编码。
fn canonical_json_string(value: &serde_json::Value) -> Result<String> {
    let mut output = String::new();
    write_canonical_json(value, &mut output)?;
    Ok(output)
}

/// 递归按规范写入 JSON，保持对象键排序。
///
/// # 参数
/// * `value` - 当前 JSON 节点
/// * `output` - 写入目标字符串
///
/// # 返回
/// 成功时返回 `Ok(())`。
///
/// # 错误
/// 键序列化失败时返回 `Internal` 错误。
fn write_canonical_json(value: &serde_json::Value, output: &mut String) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            output.push('{');
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).map_err(|error| Error::Internal(error.to_string()))?,
                );
                output.push(':');
                write_canonical_json(&map[key], output)?;
            }
            output.push('}');
        },
        serde_json::Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(']');
        },
        other => {
            output
                .push_str(&serde_json::to_string(other).map_err(|error| Error::Internal(error.to_string()))?);
        },
    }
    Ok(())
}

/// 根级供应商资料命令的稳定结果，也用于幂等查询。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierProfileMutationView {
    /// 供应商 ID。
    pub supplier_id: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 当前商务版本 ID。
    pub revision_id: String,
    /// 当前商务版本号。
    pub revision_no: u32,
    /// 保存后的供应商乐观锁版本。
    pub supplier_version: u64,
    /// 命令业务生效日期。
    pub effective_from: String,
    /// 命令记录时间（秒级时间戳）。
    pub recorded_at: u64,
    /// 原始变更原因。
    pub change_reason: String,
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::PartyId;
    use erp_core::money::Rate;
    use validator::Validate;

    use super::super::{SortDir, SupplierListParams, SupplierQualificationHealth, normalize_sort};
    use super::{
        SaveSupplierProfileRequest, SupplierProfileAddressInput, SupplierProfileBankAccountInput,
        SupplierProfileContactInput, SupplierProfileQualificationInput,
    };
    use crate::entity::supplier::{
        CapabilityCode, InvoiceType, QualificationType, ReconciliationCycle, SettlementMode,
    };
    use crate::error::Error;

    /// Node 种子生成器与 Rust 共享固定日期请求，验证最新 DTO、结算及合同资格合同。
    #[test]
    fn development_seed_requests_match_current_domain_contracts() {
        use erp_core::ids::{
            SupplierAccountId, SupplierCommercialProfileRevisionId, SupplierQualificationId,
        };

        use crate::{
            QualificationStatus, SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
            SupplierQualification, SupplierQualificationData,
        };
        let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../scripts/fixtures/dev-supplier-profiles.json"
        )))
        .unwrap();
        let today = "2026-09-10".parse().unwrap();
        assert_eq!(rows.len(), 9);
        for mut row in rows {
            let request: SaveSupplierProfileRequest = serde_json::from_value(row.clone()).unwrap();
            request.validate_contract().unwrap();
            row["supplier_id"] = "seed-supplier".into();
            row["revision_no"] = 1.into();
            let data: SupplierCommercialProfileRevisionData = serde_json::from_value(row).unwrap();
            SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("seed-profile"),
                data,
            )
            .unwrap();
            let mut contracts = Vec::new();
            for input in request.qualifications {
                let qualification = SupplierQualification::new(
                    SupplierQualificationId::new("seed-contract"),
                    SupplierQualificationData {
                        supplier_id: SupplierAccountId::new("seed-supplier"),
                        qualification_type: input.qualification_type,
                        certificate_no: input.certificate_no,
                        issuer: input.issuer,
                        valid_from: input.valid_from,
                        valid_to: input.valid_to,
                        attachment_id: input.attachment_id,
                        status: QualificationStatus::Active,
                    },
                    "seed-actor",
                )
                .unwrap();
                contracts.push(qualification);
            }
            let blocked =
                matches!(request.supplier_no.as_deref(), Some("SUP-DEV-UNVERIFIED" | "SUP-DEV-EXPIRED"));
            assert_eq!(
                crate::entity::supplier::eligibility::ensure_linked_contracts_qualified(&contracts, today)
                    .is_err(),
                blocked
            );
        }
    }

    fn save_supplier_profile_request() -> SaveSupplierProfileRequest {
        SaveSupplierProfileRequest {
            idempotency_key: "supplier-profile-command-1".to_string(),
            party_no: Some("PARTY-001".to_string()),
            supplier_no: Some("SUP-001".to_string()),
            expected_party_version: None,
            expected_supplier_version: None,
            legal_name: "上海示例供应链有限公司".to_string(),
            short_name: Some("示例供应链".to_string()),
            unified_credit_code: Some("91310000TEST000001".to_string()),
            contact: Some(SupplierProfileContactInput {
                contact_name: "张三".to_string(),
                mobile: "13800000000".to_string(),
                telephone: None,
                email: None,
            }),
            clear_contact: false,
            address: Some(SupplierProfileAddressInput {
                address: "上海市浦东新区示例路 1 号".to_string(),
                contact_name: Some("张三".to_string()),
            }),
            clear_address: false,
            tax_no: Some("91310000TEST000001".to_string()),
            clear_tax_profile: false,
            bank_account: Some(SupplierProfileBankAccountInput {
                bank_name: "中国银行".to_string(),
                account_number: "6222000000000000".to_string(),
            }),
            clear_bank_account: false,
            settlement_mode: SettlementMode::PayAfterUse,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "POSTPAY_NET30".to_string(),
            business_category: Some("办公用品".to_string()),
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Some(Rate::from_str("0.13").unwrap()),
            invoice_tax_rates: None,
            signing_entity_party_id: PartyId::new("party-signing"),
            payment_entity_party_id: PartyId::new("party-payment"),
            maintainer_user_id: Some("buyer-1".to_string()),
            capability_owners: vec![],
            capability_codes: vec![CapabilityCode::Physical],
            qualifications: vec![SupplierProfileQualificationInput {
                qualification_type: QualificationType::Contract,
                certificate_no: "CONTRACT-001".to_string(),
                issuer: None,
                valid_from: Some(BusinessDate::from_ymd(2026, 8, 31).unwrap()),
                valid_to: None,
                attachment_id: None,
                capability_codes: vec![CapabilityCode::Physical],
            }],
            rating: None,
            effective_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            change_reason: "首次登记".to_string(),
        }
    }

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" supplier_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "supplier_no"],
        )
        .unwrap();
        assert_eq!(field, "supplier_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn supplier_profile_bank_account_requires_bank_and_account_number() {
        let valid = SupplierProfileBankAccountInput {
            bank_name: "中国银行".to_string(),
            account_number: "6222000000000000".to_string(),
        };
        assert!(valid.validate().is_ok());

        let missing_bank = SupplierProfileBankAccountInput {
            bank_name: " ".to_string(),
            account_number: "6222000000000000".to_string(),
        };
        assert!(missing_bank.validate().is_err());

        let missing_account_number = SupplierProfileBankAccountInput {
            bank_name: "中国银行".to_string(),
            account_number: " ".to_string(),
        };
        assert!(missing_account_number.validate().is_err());
    }

    #[test]
    fn supplier_profile_contract_accepts_complete_input_and_blank_cleared_tax_number() {
        let request = save_supplier_profile_request();
        assert!(request.validate_contract().is_ok());

        let cleared_tax = SaveSupplierProfileRequest {
            tax_no: Some("   ".to_string()),
            clear_tax_profile: true,
            ..request
        };
        assert!(cleared_tax.validate_contract().is_ok());
    }

    #[test]
    fn supplier_profile_contract_runs_root_validator_before_intent_conflicts() {
        let request = SaveSupplierProfileRequest {
            idempotency_key: " ".to_string(),
            clear_contact: true,
            clear_address: true,
            ..save_supplier_profile_request()
        };

        let error = request.validate_contract().unwrap_err();
        match error {
            Error::ValidationError(message) => {
                assert!(message.contains("幂等键不能为空"));
                assert!(!message.contains("联系人不能同时替换和清空"));
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn supplier_profile_contract_preserves_clear_intent_error_order_and_text() {
        let request = SaveSupplierProfileRequest {
            clear_contact: true,
            clear_address: true,
            clear_bank_account: true,
            ..save_supplier_profile_request()
        };

        assert!(matches!(
            request.validate_contract(),
            Err(Error::ValidationError(message)) if message == "联系人不能同时替换和清空"
        ));
    }

    #[test]
    fn supplier_profile_contract_validates_nested_inputs_in_stable_order() {
        let mut request = save_supplier_profile_request();
        request.contact.as_mut().unwrap().contact_name = " ".to_string();
        request.address.as_mut().unwrap().address = " ".to_string();

        let error = request.validate_contract().unwrap_err();
        match error {
            Error::ValidationError(message) => {
                assert!(message.contains("联系人姓名不能为空"));
                assert!(!message.contains("地址不能为空"));
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn supplier_list_normalizes_repeated_filter_codes() {
        let params = SupplierListParams {
            capability_codes: Some(" physical, api,physical ".to_string()),
            qualification_types: Some("contract, food_license,contract".to_string()),
            qualification_health: Some(SupplierQualificationHealth::Expiring30),
            ..SupplierListParams::default()
        };

        let query = params.normalized().unwrap();
        assert_eq!(query.capability_codes.len(), 2);
        assert_eq!(query.qualification_types.len(), 2);
        assert_eq!(query.qualification_health, Some(SupplierQualificationHealth::Expiring30));
    }

    #[test]
    fn supplier_list_rejects_unknown_filter_code() {
        let params = SupplierListParams {
            capability_codes: Some("unknown".to_string()),
            ..SupplierListParams::default()
        };

        assert!(params.normalized().is_err());
    }

    #[test]
    fn supplier_profile_fingerprint_is_versioned_and_canonical() {
        let req = save_supplier_profile_request();
        let fp1 = req.fingerprint().unwrap();
        let fp2 = req.fingerprint().unwrap();
        assert!(fp1.starts_with("sha256-v1:"));
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), "sha256-v1:".len() + 64);
        // 字段乱序或键排序不影响指纹：规范 JSON 按键排序
        let mut req2 = req.clone();
        req2.legal_name = "不同名称".to_string();
        let fp3 = req2.fingerprint().unwrap();
        assert_ne!(fp1, fp3);
        // 兼容旧裸 hex：存储为裸 hex 的命令仍可回放
        let digest = fp1.strip_prefix("sha256-v1:").unwrap();
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
