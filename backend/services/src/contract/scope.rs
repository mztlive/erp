//! 合同列表的客户归属可见范围。
//!
//! 跨域只读 D08 `customer_assignments` Repository，不调用 CustomerService。
//! `assigned` 范围与销售单创建的 `ensure_customer_access` 一致：当前用户作为
//! OWNER 或 COLLABORATOR 的有效归属客户。

use database::{CustomerExt, NoTransaction};
use entities::common::time::BusinessDate;

use crate::errors::Result;

use super::dto::ContractListScope;
use super::ContractService;

impl ContractService {
    /// 按列表范围解析允许返回的客户 ID 集合。
    ///
    /// # 参数
    /// * `scope` - 客户归属可见范围
    /// * `requested_customer_id` - 调用方显式指定的单个客户；`None` 表示不限单个客户
    /// * `actor_user_id` - 当前登录用户 ID，用于读取有效归属
    ///
    /// # 返回
    /// * `None` - 不按客户 ID 过滤
    /// * `Some(ids)` - 仅这些客户；空向量表示没有任何可见客户
    ///
    /// # 错误
    /// * `RepositoryError` - 读取归属失败
    ///
    /// # 约束
    /// `Assigned` 与显式 `customer_id` 同时存在时取交集；客户不在归属内时返回空集合。
    pub(super) async fn visible_customer_ids(
        &self,
        scope: ContractListScope,
        requested_customer_id: Option<String>,
        actor_user_id: &str,
    ) -> Result<Option<Vec<String>>> {
        let assigned_customer_ids = match scope {
            ContractListScope::All => None,
            ContractListScope::Assigned => Some(self.assigned_customer_ids(actor_user_id).await?),
        };
        Ok(intersect_customer_ids(
            requested_customer_id,
            assigned_customer_ids,
        ))
    }

    /// 读取当前用户在今天仍有效的归属客户 ID。
    ///
    /// # 参数
    /// * `actor_user_id` - 当前登录用户 ID
    ///
    /// # 返回
    /// 返回 OWNER / COLLABORATOR 有效归属对应的客户 ID；无归属时为空向量。
    ///
    /// # 错误
    /// * `RepositoryError` - 读取归属失败
    ///
    /// # 约束
    /// 有效期按业务日 `BusinessDate::today()`（UTC 自然日）判定，与客户列表范围一致。
    async fn assigned_customer_ids(&self, actor_user_id: &str) -> Result<Vec<String>> {
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor_user_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        Ok(assignments
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect())
    }
}

/// 将请求中的客户筛选与当前用户归属范围求交。
///
/// # 参数
/// * `requested_customer_id` - 调用方显式指定的客户；`None` 表示不限单个客户
/// * `assigned_customer_ids` - 当前用户有效归属客户；`None` 表示不按归属收窄
///
/// # 返回
/// * `None` - 不按客户 ID 过滤
/// * `Some(ids)` - 仅这些客户；空向量表示没有任何可见客户
///
/// # 错误
/// 无。
///
/// # 约束
/// 请求客户不在归属集合内时返回空向量，禁止退回全量合同。
fn intersect_customer_ids(
    requested_customer_id: Option<String>,
    assigned_customer_ids: Option<Vec<String>>,
) -> Option<Vec<String>> {
    match (requested_customer_id, assigned_customer_ids) {
        (None, None) => None,
        (Some(customer_id), None) => Some(vec![customer_id]),
        (None, Some(assigned)) => Some(assigned),
        (Some(customer_id), Some(assigned)) => {
            if assigned.iter().any(|id| id == &customer_id) {
                Some(vec![customer_id])
            } else {
                Some(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::intersect_customer_ids;

    #[test]
    fn intersect_keeps_unscoped_request_or_assignment() {
        assert_eq!(intersect_customer_ids(None, None), None);
        assert_eq!(
            intersect_customer_ids(Some("cust-1".to_string()), None),
            Some(vec!["cust-1".to_string()])
        );
        assert_eq!(
            intersect_customer_ids(None, Some(vec!["cust-1".to_string(), "cust-2".to_string()])),
            Some(vec!["cust-1".to_string(), "cust-2".to_string()])
        );
    }

    #[test]
    fn intersect_drops_unassigned_requested_customer() {
        assert_eq!(
            intersect_customer_ids(
                Some("cust-1".to_string()),
                Some(vec!["cust-1".to_string(), "cust-2".to_string()]),
            ),
            Some(vec!["cust-1".to_string()])
        );
        assert_eq!(
            intersect_customer_ids(Some("cust-9".to_string()), Some(vec!["cust-1".to_string()])),
            Some(Vec::new())
        );
        assert_eq!(
            intersect_customer_ids(Some("cust-1".to_string()), Some(Vec::new())),
            Some(Vec::new())
        );
    }
}
