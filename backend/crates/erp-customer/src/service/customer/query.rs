//! 客户列表与详情：统一消费 DataScope 快照。

use application_core::AuditActor;

use super::scope::{
    ensure_page, ensure_scope_version, ensure_stable_snapshot, CustomerListView, CustomerSnapshot,
};
use super::CustomerService;
use crate::dto::customer::{CustomerDetailView, CustomerListParams, CustomerView};
use crate::error::Result;
use validator::Validate;

impl CustomerService {
    /// 分页查询客户角色列表。
    ///
    /// 授权、筛选、计数和负责人候选在同一查询快照中完成。
    ///
    /// # 参数
    /// * `params` - 查询参数，含范围版本与组织筛选
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页视图及 `scope_summary`、`as_of` 与权限／组织／范围版本。
    ///
    /// # 错误
    /// * `ValidationError` - 分页或筛选非法
    /// * `ConflictError` - 跨页缺版本或范围已变化
    /// * `Forbidden` - 没有 list 动作
    ///
    /// # 关键业务约束
    /// 角色无有效范围返回空集并标记 `no_scope`；有规则但对象为空不使用该标记。不得用公司范围兜底。
    pub async fn customer_list(
        &self,
        params: &CustomerListParams,
        actor: &AuditActor,
    ) -> Result<CustomerListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self.list_snapshot(params, query.clone(), actor).await?;
        ensure_scope_version(params.scope_version.as_deref(), &snapshot.context.scope_version)?;
        let current = self.list_snapshot(params, query, actor).await?;
        ensure_stable_snapshot(&snapshot.context.scope_version, &current.context.scope_version)?;
        Ok(to_list_view(snapshot))
    }

    /// 查询客户角色详情（客户 + 主体身份 + 当前生效 OWNER）。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回客户详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 客户不存在或不在读取范围内
    /// * `ConflictError` - 查询过程中范围或客户版本变化
    ///
    /// # 关键业务约束
    /// 详情必须独立重验当前权限；历史参与只补充读取。
    pub async fn customer_detail(&self, id: &str, actor: &AuditActor) -> Result<CustomerDetailView> {
        let access = self.access();
        let expected = access.require(actor, "detail", id).await?;
        let view = self.load_customer_detail(id).await?;
        let current = access.require(actor, "detail", id).await?;
        ensure_stable_snapshot(&expected.scope_version, &current.scope_version)?;
        Ok(view)
    }

    /// 装载详情展示字段，不解释数据范围。
    ///
    /// HTTP 详情入口必须先调用 `customer_detail`；跨域事实端口在入口已证明对象资格后可调用本方法。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    ///
    /// # 返回
    /// 返回客户详情视图。
    ///
    /// # 错误
    /// 客户不存在时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 本方法不解释权限，避免仓储按登录人推断范围。
    pub async fn load_customer_detail(&self, id: &str) -> Result<CustomerDetailView> {
        let account = self.load_customer(id).await?;
        let identity = self
            .party
            .identities_by_ids(std::slice::from_ref(&account.party_id))
            .await?
            .into_iter()
            .next();
        let party_no = identity.as_ref().map(|fact| fact.party_no.clone());
        let legal_name = identity.as_ref().and_then(|fact| fact.legal_name.clone());
        let owner_user_id = self.current_owner_user_id(&account.base.id).await?;
        let owner_user_name = match owner_user_id.as_deref() {
            Some(user_id) => self
                .accounts
                .names_by_ids(&[user_id.to_string()])
                .await?
                .remove(user_id),
            None => None,
        };
        let mut view: CustomerView = account.into();
        view.party_no = party_no.clone();
        view.legal_name = legal_name.clone();
        view.owner_user_id = owner_user_id.clone();
        view.owner_user_name = owner_user_name;
        Ok(CustomerDetailView {
            account: view,
            party_no,
            legal_name,
            owner_user_id,
        })
    }
}

/// 将内部快照转换为对外列表视图。
///
/// # 参数
/// * `snapshot` - 同一事务读取的授权与业务快照
///
/// # 返回
/// 返回可序列化的列表响应。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不把内部授权证明或全量人员集合返回给客户端。
fn to_list_view(snapshot: CustomerSnapshot) -> CustomerListView {
    CustomerListView {
        scope_version: snapshot.context.scope_version,
        policy_version: snapshot.context.policy_version,
        organization_version: snapshot.context.organization_version,
        as_of: snapshot.context.as_of.as_utc().to_rfc3339(),
        empty_reason: snapshot.no_scope.then_some("no_scope"),
        scope_summary: "客户当前主负责人、协作关系及负责人所属组织范围",
        data: application_core::FilteredPage {
            owner_options: snapshot.owner_options,
            ownership_basis: "current_customer_owner",
            page: application_core::PageView {
                items: snapshot.items,
                total: snapshot.total,
                page: snapshot.page,
                page_size: snapshot.page_size,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::scope::{ensure_page, ensure_scope_version, ensure_stable_snapshot};
    use crate::error::Error;

    #[test]
    fn later_page_and_version_drift_are_data_scope_changed() {
        match ensure_page(3, None) {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            }
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_scope_version(Some("scope-a"), "scope-b") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            }
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_stable_snapshot("scope-a", "scope-b") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            }
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        assert!(ensure_scope_version(Some("scope-a"), "scope-a").is_ok());
        assert!(ensure_stable_snapshot("scope-a", "scope-a").is_ok());
    }
}
