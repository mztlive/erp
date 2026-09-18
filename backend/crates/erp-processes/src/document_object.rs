//! 审批对象读取的共用空值关闭。
//!
//! 各单据 `*_object_readable` 对组织与审批人做同一 trim 非空检查；本模块只收敛
//! 这段机械判定。函数名仍留在各自 adapter，供 include_str! 守卫扫描。

use crate::{Error, Result};

/// 判定单据组织与审批人均非空。
///
/// # 参数
/// * `organization_id` - 单据责任组织
/// * `assignee_user_id` - 指定审批人
///
/// # 返回
/// 组织与审批人均非空时允许读取。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
///
/// # 关键业务约束
/// 未提供组织或审批人时失败关闭，不得默认放行。
pub(crate) fn document_object_readable(organization_id: &str, assignee_user_id: &str) -> Result<bool> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_org_or_assignee() {
        assert!(document_object_readable("org-1", "u1").unwrap());
        assert!(document_object_readable(" ", "u1").is_err());
        assert!(document_object_readable("org-1", "").is_err());
    }
}
