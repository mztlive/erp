//! `service_evidence`：线下服务履约确认的证据值对象（实际服务地点与现场图片
//! 凭证元数据策略）。
//!
//! 数据模型 §6.7 要求确认命令把采购审核占位草稿替换为实际服务地点，并登记
//! 敏感、长期保留的现场图片凭证。本模块持有两套无 I/O 的纯规则：
//! - [`ActualServiceLocation`]：地点 trim 与占位值/空白拒绝，只向 Service
//!   输出已规范化明文；加密与查询指纹仍由 Service 完成（§4.5.5，密钥不得
//!   进入实体层）；
//! - [`ServiceEvidencePolicy`]：现场图片凭证的 MIME 白名单、敏感级别、保留
//!   策略与销毁状态校验，待登记与既有资产两条确认路径必须使用同一策略。
//!
//! 错误信息为固定文案，不携带地点明文或文件敏感元数据。

use crate::errors::{Error, Result};
use crate::file_asset::{RetentionClass, SensitivityClass};

/// 采购审核草稿使用的服务地点占位值；确认时必须替换为实际地点。
pub const SERVICE_LOCATION_PLACEHOLDER: &str = "待填写";

/// 已规范化并校验通过的实际服务地点。
///
/// 只承载去除首尾空白、非空且不是采购审核占位值的明文；地点加密值与查询
/// 指纹由 Service 使用 [`crate::fulfillment::ServiceFulfillment`] 的指纹函数
/// 与加密编解码器计算，本值对象不接触密钥或密文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActualServiceLocation(String);

impl ActualServiceLocation {
    /// 解析并规范化实际服务地点。
    ///
    /// # 参数
    /// * `raw` - 确认命令中的原始服务地点
    ///
    /// # 返回
    /// 返回去除首尾空白且非占位值的实际地点。
    ///
    /// # 错误
    /// 空白或仍为「待填写」占位值时返回 `LogicError`；错误文案不包含输入
    /// 明文，避免地点敏感值进入日志。
    pub fn parse(raw: &str) -> Result<Self> {
        let service_location = raw.trim();
        if service_location.is_empty() {
            return Err(Error::from("服务地点不能为空"));
        }
        if service_location == SERVICE_LOCATION_PLACEHOLDER {
            return Err(Error::from("请填写实际服务地点"));
        }
        Ok(Self(service_location.to_string()))
    }

    /// 返回规范化后的地点明文引用。
    ///
    /// # 返回
    /// 返回去除首尾空白的服务地点字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 现场图片凭证元数据策略（数据模型 §6.7）。
///
/// 固定允许 `image/jpeg`、`image/png`、`image/webp`，且必须按敏感文件保存、
/// 长期保留、未被销毁。待登记（multipart 本批次）与既有资产（已持久化
/// `file_asset`）两条确认路径必须调用同一策略，禁止在 Service 复制规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServiceEvidencePolicy;

impl ServiceEvidencePolicy {
    /// 校验现场图片凭证元数据。
    ///
    /// # 参数
    /// * `content_type` - 文件 MIME 类型
    /// * `sensitivity` - 敏感级别
    /// * `retention` - 保留策略
    /// * `destroyed` - 是否已销毁
    ///
    /// # 返回
    /// 元数据合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 非图片、非敏感、非长期保留或已销毁时返回 `LogicError`；错误文案为
    /// 固定提示，不包含文件敏感元数据。
    pub fn validate(
        content_type: &str,
        sensitivity: SensitivityClass,
        retention: RetentionClass,
        destroyed: bool,
    ) -> Result<()> {
        if !matches!(content_type, "image/jpeg" | "image/png" | "image/webp") {
            return Err(Error::from("现场凭证仅支持 JPG、PNG 或 WebP 图片"));
        }
        if sensitivity == SensitivityClass::General {
            return Err(Error::from("现场图片凭证必须按敏感文件保存"));
        }
        if retention != RetentionClass::LongTerm {
            return Err(Error::from("现场图片凭证必须长期保留"));
        }
        if destroyed {
            return Err(Error::from("现场图片凭证已销毁"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ActualServiceLocation, ServiceEvidencePolicy, SERVICE_LOCATION_PLACEHOLDER};
    use crate::file_asset::{RetentionClass, SensitivityClass};

    /// 合法地点去除首尾空白并稳定保留内部字符。
    #[test]
    fn actual_service_location_trims_and_keeps_value() {
        assert_eq!(
            ActualServiceLocation::parse(" 客户现场 ").unwrap().as_str(),
            "客户现场"
        );
        assert_eq!(
            ActualServiceLocation::parse("\t\n 客户现场 1 号 \n")
                .unwrap()
                .as_str(),
            "客户现场 1 号"
        );
        assert_eq!(
            ActualServiceLocation::parse("客户现场").unwrap().as_str(),
            "客户现场"
        );
    }

    /// 空白与采购审核占位值（含带空白变体）必须拒绝，且错误不携带明文。
    #[test]
    fn actual_service_location_rejects_blank_and_placeholder() {
        for raw in ["", "   ", "\t\n"] {
            let error = ActualServiceLocation::parse(raw).unwrap_err();
            assert_eq!(error.to_string(), "服务地点不能为空");
        }
        for raw in [SERVICE_LOCATION_PLACEHOLDER, " 待填写 ", "\t待填写\n"] {
            let error = ActualServiceLocation::parse(raw).unwrap_err();
            assert_eq!(error.to_string(), "请填写实际服务地点");
            assert!(!error.to_string().contains(raw.trim()));
        }
    }

    /// 策略接受白名单图片与敏感/高敏感、长期保留、未销毁组合。
    #[test]
    fn policy_accepts_supported_image_metadata() {
        for content_type in ["image/jpeg", "image/png", "image/webp"] {
            for sensitivity in [SensitivityClass::Sensitive, SensitivityClass::HighlySensitive] {
                assert!(
                    ServiceEvidencePolicy::validate(
                        content_type,
                        sensitivity,
                        RetentionClass::LongTerm,
                        false,
                    )
                    .is_ok(),
                    "{content_type} {sensitivity:?} 应通过"
                );
            }
        }
    }

    /// 白名单之外的 MIME 一律拒绝（含空串与近似变体）。
    #[test]
    fn policy_rejects_non_image_mime() {
        for content_type in ["application/pdf", "image/gif", "image/jpg", "text/plain", ""] {
            let error = ServiceEvidencePolicy::validate(
                content_type,
                SensitivityClass::Sensitive,
                RetentionClass::LongTerm,
                false,
            )
            .unwrap_err();
            assert_eq!(error.to_string(), "现场凭证仅支持 JPG、PNG 或 WebP 图片");
        }
    }

    /// General 敏感级别拒绝，Sensitive/HighlySensitive 通过。
    #[test]
    fn policy_rejects_general_sensitivity() {
        let error = ServiceEvidencePolicy::validate(
            "image/jpeg",
            SensitivityClass::General,
            RetentionClass::LongTerm,
            false,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "现场图片凭证必须按敏感文件保存");
        assert!(ServiceEvidencePolicy::validate(
            "image/jpeg",
            SensitivityClass::Sensitive,
            RetentionClass::LongTerm,
            false,
        )
        .is_ok());
        assert!(ServiceEvidencePolicy::validate(
            "image/jpeg",
            SensitivityClass::HighlySensitive,
            RetentionClass::LongTerm,
            false,
        )
        .is_ok());
    }

    /// 非长期保留策略拒绝（30 天、7 天均失败关闭）。
    #[test]
    fn policy_rejects_non_long_term_retention() {
        for retention in [RetentionClass::ThirtyDays, RetentionClass::SevenDays] {
            let error =
                ServiceEvidencePolicy::validate("image/png", SensitivityClass::Sensitive, retention, false)
                    .unwrap_err();
            assert_eq!(error.to_string(), "现场图片凭证必须长期保留");
        }
    }

    /// 已销毁凭证拒绝。
    #[test]
    fn policy_rejects_destroyed() {
        let error = ServiceEvidencePolicy::validate(
            "image/webp",
            SensitivityClass::Sensitive,
            RetentionClass::LongTerm,
            true,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "现场图片凭证已销毁");
    }
}
