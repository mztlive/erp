//! 公开的展示映射工具；保留调用方自定义映射表的查找与缺省语义。
//! 本域固定枚举使用穷举匹配，新增变体时由编译器检查映射是否完整。

/// 枚举展示映射表的一行：变体标签与稳定代码。
pub struct DisplayEntry<T> {
    /// 枚举变体。
    pub variant: T,
    /// 中文展示名。
    pub label: &'static str,
    /// 稳定持久化代码。
    pub code: &'static str,
}

/// 由映射表实现 `label()` 查找。
/// 调用方应保证变体在表中；表中没有该变体时不 panic。
/// 命中时返回对应中文标签；未命中时返回空字符串。
pub fn label_of<T: Copy + PartialEq>(variant: T, table: &[DisplayEntry<T>]) -> &'static str {
    table.iter().find(|entry| entry.variant == variant).map(|entry| entry.label).unwrap_or("")
}

/// 由映射表实现 `as_str()` 查找。
/// 调用方应保证变体在表中；表中没有该变体时不 panic。
/// 命中时返回对应稳定代码；未命中时返回空字符串。
pub fn code_of<T: Copy + PartialEq>(variant: T, table: &[DisplayEntry<T>]) -> &'static str {
    table.iter().find(|entry| entry.variant == variant).map(|entry| entry.code).unwrap_or("")
}

/// 按稳定代码识别枚举变体，让调用方区分已登记枚举值和无法识别的输入。
/// 命中时返回对应变体；未知代码返回 `None`。
pub fn from_code<T: Copy>(code: &str, table: &[DisplayEntry<T>]) -> Option<T> {
    table.iter().find(|entry| entry.code == code).map(|entry| entry.variant)
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::{DisplayEntry, code_of, from_code, label_of};
    use crate::entity::supplier::{
        CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
        ReconciliationCycle, SettlementMode, SupplierAccountStatus,
    };

    fn assert_enum_mapping<T>(
        cases: &[(T, &str, &str)],
        label: impl Fn(&T) -> &'static str,
        code: impl Fn(&T) -> &'static str,
    ) where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        for (variant, expected_code, expected_label) in cases {
            assert_eq!(code(variant), *expected_code);
            assert_eq!(label(variant), *expected_label);
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, serde_json::to_string(expected_code).unwrap());
            assert_eq!(serde_json::from_str::<T>(&json).unwrap(), *variant);
        }
    }

    #[test]
    fn settlement_and_reconciliation_codes_keep_their_labels_and_wire_values() {
        assert_enum_mapping(
            &[
                (SettlementMode::Prepayment, "prepayment", "预付款"),
                (SettlementMode::PayAfterUse, "pay_after_use", "货到后付（历史）"),
                (SettlementMode::Weekly, "weekly", "周结"),
                (SettlementMode::Monthly, "monthly", "月结"),
                (SettlementMode::Quarterly, "quarterly", "季结"),
                (SettlementMode::HalfYearly, "half_yearly", "半年结"),
                (SettlementMode::Yearly, "yearly", "年结"),
                (SettlementMode::CashSettlement, "cash_settlement", "现结"),
            ],
            SettlementMode::label,
            SettlementMode::as_str,
        );

        assert_enum_mapping(
            &[
                (ReconciliationCycle::Daily, "daily", "日"),
                (ReconciliationCycle::Weekly, "weekly", "周"),
                (ReconciliationCycle::Monthly, "monthly", "月"),
                (ReconciliationCycle::Quarterly, "quarterly", "季"),
                (ReconciliationCycle::Yearly, "yearly", "年"),
                (ReconciliationCycle::HalfYearly, "half_yearly", "半年"),
                (ReconciliationCycle::None, "none", "无需周期对账"),
            ],
            ReconciliationCycle::label,
            ReconciliationCycle::as_str,
        );
    }

    #[test]
    fn invoice_and_status_codes_keep_their_labels_and_wire_values() {
        assert_enum_mapping(
            &[
                (InvoiceType::VatSpecial, "vat_special", "增值税专用发票"),
                (InvoiceType::VatNormal, "vat_normal", "增值税普通发票"),
                (InvoiceType::Electronic, "electronic", "电子发票"),
            ],
            InvoiceType::label,
            InvoiceType::as_str,
        );
        assert_enum_mapping(
            &[
                (SupplierAccountStatus::Active, "active", "启用"),
                (SupplierAccountStatus::Disabled, "disabled", "停用"),
            ],
            SupplierAccountStatus::label,
            SupplierAccountStatus::as_str,
        );
        assert_enum_mapping(
            &[(CapabilityStatus::Active, "active", "启用"), (CapabilityStatus::Disabled, "disabled", "停用")],
            CapabilityStatus::label,
            CapabilityStatus::as_str,
        );
        assert_enum_mapping(
            &[
                (QualificationStatus::Active, "active", "有效"),
                (QualificationStatus::Expired, "expired", "失效"),
                (QualificationStatus::Disabled, "disabled", "停用"),
            ],
            QualificationStatus::label,
            QualificationStatus::as_str,
        );
    }

    #[test]
    fn capability_and_qualification_parsing_keeps_exact_code_matching() {
        let capabilities = [
            (CapabilityCode::Physical, "physical", "实物商品"),
            (CapabilityCode::Virtual, "virtual", "虚拟商品"),
            (CapabilityCode::OfflineService, "offline_service", "线下服务"),
            (CapabilityCode::Api, "api", "API"),
            (CapabilityCode::Printing, "printing", "印刷"),
        ];
        assert_enum_mapping(&capabilities, CapabilityCode::label, CapabilityCode::as_str);
        for (variant, code, _) in capabilities {
            assert_eq!(CapabilityCode::from_code(code), Some(variant));
        }

        let qualifications = [
            (QualificationType::Certificate, "certificate", "资质证照"),
            (QualificationType::Contract, "contract", "合同"),
            (QualificationType::Authorization, "authorization", "授权书"),
            (QualificationType::FoodLicense, "food_license", "食品经营许可证"),
            (QualificationType::LegalPersonId, "legal_person_id", "法人身份证"),
        ];
        assert_enum_mapping(&qualifications, QualificationType::label, QualificationType::as_str);
        for (variant, code, _) in qualifications {
            assert_eq!(QualificationType::from_code(code), Some(variant));
        }

        for unknown in ["", "PHYSICAL", " physical ", "CONTRACT", " contract ", "unknown"] {
            assert_eq!(CapabilityCode::from_code(unknown), None);
            assert_eq!(QualificationType::from_code(unknown), None);
        }
    }

    #[test]
    fn public_table_helpers_preserve_first_match_and_missing_entry_results() {
        let table = [
            DisplayEntry { variant: 1, label: "首项", code: "first" },
            DisplayEntry { variant: 1, label: "重复项", code: "second" },
            DisplayEntry { variant: 2, label: "相同代码", code: "first" },
        ];
        assert_eq!(label_of(1, &table), "首项");
        assert_eq!(code_of(1, &table), "first");
        assert_eq!(from_code("first", &table), Some(1));
        assert_eq!(label_of(3, &table), "");
        assert_eq!(code_of(3, &table), "");
        assert_eq!(from_code("unknown", &table), None);
        assert_eq!(label_of(1, &[]), "");
        assert_eq!(code_of(1, &[]), "");
        assert_eq!(from_code::<i32>("first", &[]), None);
    }
}
