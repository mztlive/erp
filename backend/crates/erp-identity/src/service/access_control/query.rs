//! 范围配置列表查询；信封版本与空集字段与组织查询同口径。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use persistence_core::Transactional;
use validator::Validate;

use super::{AccessControlService, DataScopeFilter};
use crate::AccessControlExt;
use crate::dto::{DataScopeListMeta, DataScopeListParams, DataScopeListView, DataScopeView, PageView};
use crate::error::{Error, Result};
use crate::repository::OrganizationRepository;

impl AccessControlService {
    /// 分页查询数据范围列表，并返回与组织查询同口径的信封。
    ///
    /// 携带 `subject_id`（与 `subject_type` 成对）时按主体批量取回；可按资源
    /// 与动作收窄，禁止通配筛选。跨页须携带上一页 `scope_version`。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 含 `items`/`total`/`page`/`page_size` 以及范围、权限、组织版本和空集原因的信封。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法、排序字段不在白名单、按主体查询缺少主体类型，或资源动作不是注册标识
    /// * `Forbidden` - 未装配范围配置授权
    /// * `ConflictError` - 跨页 `scope_version` 与当前授权不一致
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// # 关键业务约束
    /// 版本与 `empty_reason` 只出现在信封上，不得写入单条配置。缺动作由 HTTP 权限拒绝；本查询不补 Company。
    pub async fn data_scope_list(
        &self,
        actor: &AuditActor,
        params: &DataScopeListParams,
    ) -> Result<DataScopeListView> {
        params.validate()?;
        let query = params.normalized()?;
        let rbac = self.rbac.clone().ok_or_else(|| Error::Forbidden("未装配范围配置授权".into()))?;
        let db = self.db.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    let policy_version = rbac.current_policy_revision().await?;
                    rbac.ensure_policy_snapshot_with_executor(policy_version, session).await?;
                    let organizations = OrganizationRepository::new(&db).state(session).await?;
                    let as_of = Instant::now();
                    let scope_version = format!(
                        "{:x}",
                        md5::compute(
                            format!(
                                "{}:{}:{}:data_scope:list",
                                actor.id(),
                                policy_version,
                                organizations.version
                            )
                            .as_bytes()
                        )
                    );
                    if let Some(expected) = query.scope_version.as_deref()
                        && expected != scope_version
                    {
                        return Err(Error::ConflictError(
                            "DATA_SCOPE_CHANGED：数据范围已变化，请刷新".into(),
                        ));
                    }
                    let filter = DataScopeFilter {
                        subject_type: query.subject_type,
                        subject_id: query.subject_id,
                        scope_type: query.scope_type,
                        resource: query.resource,
                        action: query.action,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by: Some(query.paging.sort_by.to_string()),
                        sort_ascending: matches!(query.paging.sort_dir, crate::dto::SortDir::Asc),
                    };
                    let page = db.data_scopes().search_data_scopes(&filter, session).await?;
                    let items = page
                        .items
                        .into_iter()
                        .map(|row| DataScopeView {
                            id: row.id,
                            subject_type: row.subject_type,
                            subject_id: row.subject_id,
                            scope_type: row.scope_type,
                            scope_targets: row.scope_targets,
                            binding: row.binding,
                            version: row.version,
                            created_at: row.created_at,
                        })
                        .collect();
                    Ok::<_, Error>(DataScopeListView::compose(
                        PageView { items, total: page.total, page: filter.page, page_size: filter.page_size },
                        DataScopeListMeta {
                            scope_version,
                            policy_version,
                            organization_version: organizations.version,
                            as_of: as_of.as_utc().to_rfc3339(),
                            no_scope: false,
                        },
                    ))
                })
            })
            .await
    }
}
