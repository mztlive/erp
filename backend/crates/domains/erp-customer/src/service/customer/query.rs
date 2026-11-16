//! 客户列表与详情：统一消费 DataScope 快照。

use std::slice;

use application_core::{AuditActor, PageView};
use erp_core::common::time::{BusinessDate, Instant};
use erp_permissions::customer as customer_permissions;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CustomerService;
use super::scope::{
    CustomerListView, CustomerSnapshot, ensure_page, ensure_scope_version, ensure_stable_snapshot,
};
use crate::dto::customer::{
    CustomerDetailView, CustomerListParams, CustomerListQuery, CustomerOwnerView, CustomerView,
};
use crate::entity::customer::{CustomerAccount, CustomerAssignment};
use crate::error::{Error, Result};
use crate::repository::CustomerExt;
use crate::repository::prelude::*;
use crate::repository::scope::authorized_customer;

impl CustomerService {
    /// 分页查询客户角色列表。
    ///
    /// 授权、筛选和计数在同一查询快照中完成。负责销售候选不由本接口生产。
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
    /// 快照读取失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 角色无有效范围返回空集并标记 `no_scope`；有规则但对象为空不使用该标记。不得用公司范围兜底。
    /// 常规列表与全部有权客户共用本响应。负责人筛选只收窄客户；行负责人姓名来自当前页归属。
    pub async fn customer_list(
        &self,
        params: &CustomerListParams,
        actor: &AuditActor,
    ) -> Result<CustomerListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let service = self.clone();
        let actor = actor.clone();
        let expected_version = params.scope_version.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.list_stage(query, &actor, expected_version.as_deref(), executor).await
                })
            })
            .await
    }

    /// 在同一快照内读取列表、复核请求版本并验证交付版本。
    async fn list_stage(
        &self,
        query: CustomerListQuery,
        actor: &AuditActor,
        expected_version: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<CustomerListView> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("客户列表必须使用快照事务".into()));
        }
        let snapshot = self.list_snapshot(query.clone(), actor, executor).await?;
        ensure_scope_version(expected_version, &snapshot.context.scope_version)?;
        let current = self.list_version_snapshot(query, actor, executor).await?;
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
    /// 动作未授权或详情读取失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 详情必须独立重验当前权限；历史参与只补充读取。
    pub async fn customer_detail(&self, id: &str, actor: &AuditActor) -> Result<CustomerDetailView> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.detail_stage(&id, &actor, executor).await })
            })
            .await
    }

    /// 同一读取阶段完成完整授权过滤、详情水合和交付检查。
    async fn detail_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerDetailView> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(crate::Error::ValidationError("客户详情必须使用快照事务".into()));
        }
        let context =
            self.access.query_context(actor, &customer_permissions::DETAIL, executor).await.map_err(
                |error| match error {
                    crate::Error::Forbidden(_) => super::access::deny_object(&customer_permissions::DETAIL),
                    error => error,
                },
            )?;
        let account = authorized_customer(&self.db, id, &context, executor).await?;
        let view = self.detail_view(account, super::access::business_date(context.as_of)?, executor).await?;
        self.access.validate(&context, executor).await?;
        Ok(view)
    }

    /// 装载详情展示字段，不解释数据范围。
    ///
    /// HTTP 详情入口必须先调用 `customer_detail`；跨域事实端口在入口已证明对象资格后可调用本方法。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    /// * `executor` - 原业务事务或读取阶段执行器。
    ///
    /// # 返回
    /// 返回客户详情视图。
    ///
    /// # 错误
    /// 客户不存在时返回 `NotFound`。主体、归属或账号读取失败时返回对应错误。
    ///
    /// # 关键业务约束
    /// 本方法不解释权限，避免仓储按登录人推断范围。
    pub async fn load_customer_detail(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerDetailView> {
        let account = self
            .db
            .customer_accounts()
            .find_customer(id, executor)
            .await?
            .ok_or_else(|| crate::Error::NotFound("客户不存在".into()))?;
        self.detail_view(account, BusinessDate::today(), executor).await
    }

    /// 在原执行器补充主体、有效负责人及姓名，不开启新读取阶段。
    async fn detail_view(
        &self,
        account: CustomerAccount,
        as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<CustomerDetailView> {
        let identity = self
            .party
            .identities_by_ids(slice::from_ref(&account.party_id), executor)
            .await?
            .into_iter()
            .next();
        let party_no = identity.as_ref().map(|fact| fact.party_no.clone());
        let legal_name = identity.as_ref().and_then(|fact| fact.legal_name.clone());
        let assignments = self
            .db
            .customer_assignments()
            .list_active_for_customers(slice::from_ref(&account.base.id), as_of, executor)
            .await?;
        let owner_ids = CustomerAssignment::owner_ids(&assignments);
        let names = self.accounts.names(&owner_ids, executor).await?;
        let owner_user_id = owner_ids.first().cloned();
        let mut view: CustomerView = account.into();
        view.party_no = party_no.clone();
        view.legal_name = legal_name.clone();
        view.owner_user_id = owner_user_id.clone();
        view.owner_user_name = owner_user_id.as_ref().and_then(|id| names.get(id).cloned());
        view.owners = owner_ids
            .into_iter()
            .map(|user_id| CustomerOwnerView { user_name: names.get(&user_id).cloned(), user_id })
            .collect();
        Ok(CustomerDetailView { account: view, party_no, legal_name, owner_user_id })
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
    list_view(
        PageView {
            items: snapshot.items,
            total: snapshot.total,
            page: snapshot.page,
            page_size: snapshot.page_size,
        },
        snapshot.context.scope_version,
        snapshot.context.as_of,
    )
}

/// 用同一次授权查询的分页及版本元信息生成响应。
fn list_view(page: PageView<CustomerView>, scope_version: String, as_of: Instant) -> CustomerListView {
    CustomerListView {
        scope_version,
        as_of: as_of.as_utc().to_rfc3339(),
        empty_reason: None,
        scope_summary: "客户当前主负责人及负责人所属组织范围",
        ownership_basis: "current_customer_owner",
        data: page,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::testing::unbound_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::scope::{ensure_page, ensure_scope_version, ensure_stable_snapshot};
    use super::CustomerService;
    use crate::CustomerAccess;
    use crate::dto::customer::CustomerListParams;
    use crate::error::Error;
    use crate::ports::{FailClosedAuditPort, FailClosedPartyFactPort};

    /// 数据库句柄不发起连接，授权失败必须先于任何仓储或事实端口读取。
    async fn service() -> CustomerService {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("customer_detail_authorization");
        CustomerService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedPartyFactPort),
            CustomerAccess::new(db, unbound_authorization()),
        )
    }

    #[tokio::test]
    async fn list_rejects_non_snapshot_and_missing_binding_before_business_reads() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let params = serde_json::from_value::<CustomerListParams>(serde_json::json!({})).unwrap();
        let query = params.normalized().unwrap();
        let result = service.list_stage(query.clone(), &actor, None, &mut ReadExecutor::default()).await;
        assert!(matches!(
            result,
            Err(Error::ValidationError(message)) if message == "客户列表必须使用快照事务"
        ));
        let result = service.list_stage(query, &actor, None, &mut SnapshotExecutor::default()).await;
        assert!(matches!(
            result,
            Err(Error::ValidationError(message)) if message == "缺少查询 Mapping 绑定"
        ));
    }

    #[tokio::test]
    async fn detail_rejects_non_snapshot_before_querying_any_customer() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = service()
            .await
            .detail_stage("customer", &actor, &mut ReadExecutor::default())
            .await
            .expect_err("非快照详情读取必须拒绝");
        assert!(matches!(error, Error::ValidationError(message) if message == "客户详情必须使用快照事务"));
    }

    #[tokio::test]
    async fn detail_propagates_missing_authorization_binding_before_business_reads() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = service()
            .await
            .detail_stage("customer", &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("缺少固定授权绑定必须拒绝");
        assert!(matches!(error, Error::ValidationError(message) if message == "缺少查询 Mapping 绑定"));
    }

    #[test]
    fn later_page_and_version_drift_are_data_scope_changed() {
        match ensure_page(3, None) {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_scope_version(Some("scope-a"), "scope-b") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_stable_snapshot("scope-a", "scope-b") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        assert!(ensure_scope_version(Some("scope-a"), "scope-a").is_ok());
        assert!(ensure_stable_snapshot("scope-a", "scope-a").is_ok());
    }

    /// 常规列表与全部有权客户共用该视图：不返回负责销售候选，已授权行仍带负责人姓名。
    #[test]
    fn customer_list_omits_owner_candidates_and_keeps_row_owner_name() {
        use application_core::PageView;
        use erp_core::common::time::Instant;

        use super::list_view;
        use crate::dto::customer::{CustomerScope, CustomerView};
        use crate::entity::customer::CustomerAccountStatus;

        let view = list_view(
            PageView {
                items: vec![CustomerView {
                    id: "cust-1".to_string(),
                    party_id: "party-1".to_string(),
                    party_no: Some("P-1".to_string()),
                    legal_name: Some("示例客户".to_string()),
                    short_name: None,
                    customer_no: "C-1".to_string(),
                    default_payment_term_id: None,
                    status: CustomerAccountStatus::Active,
                    owner_user_id: Some("sales-1".to_string()),
                    owner_user_name: Some("张三".to_string()),
                    owners: Vec::new(),
                    collaborator_count: 0,
                    scope_tags: vec![CustomerScope::AllAuthorized],
                    version: 1,
                    created_at: 1,
                    updated_at: 1,
                }],
                total: 1,
                page: 1,
                page_size: 20,
            },
            "scope-v1".into(),
            Instant::from_unix_secs(0),
        );
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("owner_options").is_none());
        assert_eq!(json["ownership_basis"], "current_customer_owner");
        assert_eq!(json["total"], 1);
        assert_eq!(json["page"], 1);
        assert_eq!(json["page_size"], 20);
        assert_eq!(json["scope_version"], "scope-v1");
        assert_eq!(json["empty_reason"], serde_json::Value::Null);
        assert_eq!(json["items"][0]["owner_user_id"], "sales-1");
        assert_eq!(json["items"][0]["owner_user_name"], "张三");
    }
}
