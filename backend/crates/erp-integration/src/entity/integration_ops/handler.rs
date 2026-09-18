//! 当前处理人与所属内部组织的共享校验（INT 去重）。
//!
//! 错误任务与对账差异对处理人/组织的要求同源：拒绝 `"me"` 人员占位、
//! 拒绝 `"company"` 组织占位。空值与长度文案由调用方保留（两实体历史文案
//! 不同），本模块只收敛占位拒绝与组织规范化，避免两处重复实现。

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};

/// 处理人内部组织标识最大长度（两实体同值，唯一来源）。
pub(crate) const HANDLER_ORG_UNIT_ID_MAX_LEN: usize = 128;

/// 拒绝 `"me"` 人员占位（大小写不敏感，调用方已去空白）。
///
/// # 参数
/// * `owner` - 已规范化的处理人 ID
///
/// # 错误
/// 为 `me` 时返回领域校验错误。
pub(crate) fn reject_me_handler(owner: &str) -> Result<()> {
    if owner.eq_ignore_ascii_case("me") {
        return Err(Error::from("处理人不得使用 me 作为人员 ID"));
    }
    Ok(())
}

/// 规范化并拒绝公司占位的处理人内部组织（两实体文案相同，唯一实现）。
///
/// # 参数
/// * `raw` - 原始组织 ID
///
/// # 错误
/// 为空、超长或为 `company` 占位时返回领域校验错误。
pub(crate) fn require_handler_org_unit_id(raw: String) -> Result<String> {
    let org =
        normalize_required_text(raw, "处理人组织不能为空", HANDLER_ORG_UNIT_ID_MAX_LEN, "处理人组织过长")?;
    if org.eq_ignore_ascii_case("company") {
        return Err(Error::from("处理人组织不得使用公司占位"));
    }
    Ok(org)
}

#[cfg(test)]
mod tests {
    use super::{reject_me_handler, require_handler_org_unit_id};

    #[test]
    fn handler_guards_reject_placeholders() {
        assert!(reject_me_handler("me").is_err());
        assert!(reject_me_handler("ME").is_err());
        assert!(reject_me_handler("user-1").is_ok());
        assert_eq!(require_handler_org_unit_id(" org-a ".to_string()).unwrap(), "org-a");
        assert!(require_handler_org_unit_id("company".to_string()).is_err());
        assert!(require_handler_org_unit_id("  ".to_string()).is_err());
    }
}
