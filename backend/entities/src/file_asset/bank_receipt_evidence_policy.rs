//! `BankReceiptEvidencePolicy`：银行回单证据策略值对象（FIN-E05）。
//!
//! 银行回单作为付款证据的可用性规则（MIME、敏感级别、保留策略与销毁状态）
//! 统一归位到本策略：待登记的 pending 元数据与已落库的 stored 元数据必须
//! 通过同一入口校验，禁止在 Service 内维护两套规则。值对象不执行 I/O、
//! 不判断文件存在性；`destroyed` 布尔由 Service 按输入形态适配后传入
//! （pending 路径恒为 `false`，stored 路径为 `destroyed_at.is_some()`），
//! 校验不依赖当前时间。

use crate::errors::{Error, Result};
use crate::file_asset::{RetentionClass, SensitivityClass};

/// 银行回单证据策略。
///
/// 银行回单必须同时满足：仅接受 JPG／PNG／WebP 图片、按敏感文件保存
/// （禁止 `General`）、长期保留且未被销毁。任一条件不满足即失败关闭，
/// 错误消息稳定且保持既有错误合同（先 MIME、再敏感级别、再保留策略、
/// 最后销毁状态的首错顺序）。
pub struct BankReceiptEvidencePolicy;

impl BankReceiptEvidencePolicy {
    /// 校验银行回单证据可用（FIN-E05 唯一规则入口）。
    ///
    /// pending 路径传入登记元数据（`destroyed = false`），stored 路径传入
    /// `FileAsset` 已落库元数据（`destroyed = destroyed_at.is_some()`）；
    /// 两种输入适配得到相同结论。校验不依赖当前时间：point-in-time
    /// 过期判定不属于本策略（FIN-E10 另行承担）。
    ///
    /// # 参数
    /// * `content_type` - 文件内容类型（大小写敏感精确匹配）
    /// * `sensitivity` - 敏感级别
    /// * `retention` - 保留策略
    /// * `destroyed` - 是否已销毁（由 Service 按销毁审计时间适配）
    ///
    /// # 返回
    /// 全部规则通过时返回 `Ok(())`。
    ///
    /// # 错误
    /// MIME 不支持、敏感级别为 `General`、保留策略非长期或已销毁时返回
    /// [`Error`]；错误消息与首错顺序保持既有合同。
    ///
    /// # 约束
    /// 不执行 I/O、不判断文件存在性、不生成脱敏视图；文件存在性、临时
    /// 引用落库与批量装载仍由 Service／Repository 负责。
    pub fn validate(
        content_type: &str,
        sensitivity: SensitivityClass,
        retention: RetentionClass,
        destroyed: bool,
    ) -> Result<()> {
        if !matches!(content_type, "image/jpeg" | "image/png" | "image/webp") {
            return Err(Error::from("银行回单仅支持 JPG、PNG 或 WebP 图片"));
        }
        if sensitivity == SensitivityClass::General {
            return Err(Error::from("银行回单必须按敏感文件保存"));
        }
        if retention != RetentionClass::LongTerm {
            return Err(Error::from("银行回单必须长期保留"));
        }
        if destroyed {
            return Err(Error::from("银行回单已销毁"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{BankReceiptEvidencePolicy, RetentionClass, SensitivityClass};

    /// 允许的 MIME 与合法元数据组合全部通过。
    #[test]
    fn accepts_sensitive_long_term_images() {
        for content_type in ["image/jpeg", "image/png", "image/webp"] {
            assert!(
                BankReceiptEvidencePolicy::validate(
                    content_type,
                    SensitivityClass::Sensitive,
                    RetentionClass::LongTerm,
                    false,
                )
                .is_ok(),
                "{content_type} 应通过"
            );
            assert!(
                BankReceiptEvidencePolicy::validate(
                    content_type,
                    SensitivityClass::HighlySensitive,
                    RetentionClass::LongTerm,
                    false,
                )
                .is_ok(),
                "{content_type} 高敏感应通过"
            );
        }
    }

    /// 拒绝的 MIME 及边界形态：非图片、简写、大小写、空白均不得通过。
    #[test]
    fn rejects_unsupported_content_types() {
        for content_type in [
            "application/pdf",
            "image/gif",
            "image/jpg",
            "IMAGE/PNG",
            " image/png",
            "image/png ",
            "",
        ] {
            let error = BankReceiptEvidencePolicy::validate(
                content_type,
                SensitivityClass::Sensitive,
                RetentionClass::LongTerm,
                false,
            )
            .expect_err("应拒绝");
            assert_eq!(error.to_string(), "银行回单仅支持 JPG、PNG 或 WebP 图片");
        }
    }

    /// 敏感级别：必须为敏感或高敏感，`General` 拒绝且错误消息稳定。
    #[test]
    fn rejects_general_sensitivity() {
        let error = BankReceiptEvidencePolicy::validate(
            "image/png",
            SensitivityClass::General,
            RetentionClass::LongTerm,
            false,
        )
        .expect_err("应拒绝");
        assert_eq!(error.to_string(), "银行回单必须按敏感文件保存");
    }

    /// 保留策略：非长期保留（30 天／7 天）拒绝且错误消息稳定。
    #[test]
    fn rejects_non_long_term_retention() {
        for retention in [RetentionClass::ThirtyDays, RetentionClass::SevenDays] {
            let error = BankReceiptEvidencePolicy::validate(
                "image/png",
                SensitivityClass::Sensitive,
                retention,
                false,
            )
            .expect_err("应拒绝");
            assert_eq!(error.to_string(), "银行回单必须长期保留");
        }
    }

    /// 销毁状态：已销毁拒绝且错误消息稳定。
    #[test]
    fn rejects_destroyed_evidence() {
        let error = BankReceiptEvidencePolicy::validate(
            "image/png",
            SensitivityClass::Sensitive,
            RetentionClass::LongTerm,
            true,
        )
        .expect_err("应拒绝");
        assert_eq!(error.to_string(), "银行回单已销毁");
    }

    /// 首错顺序：多规则同时违反时固定先报 MIME，随后敏感级别，再保留策略。
    #[test]
    fn reports_first_error_in_stable_order() {
        let error = BankReceiptEvidencePolicy::validate(
            "application/pdf",
            SensitivityClass::General,
            RetentionClass::SevenDays,
            true,
        )
        .expect_err("应拒绝");
        assert_eq!(error.to_string(), "银行回单仅支持 JPG、PNG 或 WebP 图片");

        let error = BankReceiptEvidencePolicy::validate(
            "image/png",
            SensitivityClass::General,
            RetentionClass::SevenDays,
            true,
        )
        .expect_err("应拒绝");
        assert_eq!(error.to_string(), "银行回单必须按敏感文件保存");

        let error = BankReceiptEvidencePolicy::validate(
            "image/png",
            SensitivityClass::Sensitive,
            RetentionClass::SevenDays,
            true,
        )
        .expect_err("应拒绝");
        assert_eq!(error.to_string(), "银行回单必须长期保留");

        let error = BankReceiptEvidencePolicy::validate(
            "image/png",
            SensitivityClass::Sensitive,
            RetentionClass::LongTerm,
            true,
        )
        .expect_err("应拒绝");
        assert_eq!(error.to_string(), "银行回单已销毁");
    }
}
