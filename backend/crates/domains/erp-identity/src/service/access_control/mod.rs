//! 域 D06 `access_control` 服务编排。
//!
//! 覆盖人员数据范围配置、访问检查、授权文件导入导出和治理审计事件查询；
//! 账号、角色与功能权限绑定由 `service::iam` 承载。

use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::access_control;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;
use validator::Validate;

pub mod inspection;
mod person_scope;
mod person_scope_query;
pub mod policy_bundle;
pub(crate) mod resolve;

pub use crate::dto::{AuditEventListParams, AuditEventView, PageView};
use crate::entity::access_control::{AuditEvent, AuditEventData, AuditEventId, AuditEventResult};
use crate::error::{Error, Result};
use crate::ports::ScopeTargetPort;
use crate::repository::prelude::*;
use crate::service::iam::management::ManagementAccess;
use crate::{AccessControlExt, AuthorizationService};

/// 管理端访问控制用例，负责人员范围、访问检查和治理审计查询；实际角色授权由 IAM 处理。
#[derive(Clone)]
pub struct AccessControlService {
    // 身份仓储及管理用例事务使用的数据库。
    db: Database,
    // 提供角色资格快照、策略版本及策略写事务的共享 RBAC 服务。
    rbac: crate::SharedRbacService,
    // 保存人员范围前校验仓库、结算主体等外域目标的端口。
    targets: Arc<dyn ScopeTargetPort>,
    // 检查范围与审计等管理入口的统一授权服务。
    authorization: Arc<AuthorizationService>,
}

impl AccessControlService {
    /// 应用装配时注入身份数据库、RBAC、外域目标校验端口和共享授权服务；不执行 I/O。
    pub fn new(
        db: Database,
        rbac: crate::SharedRbacService,
        targets: Arc<dyn ScopeTargetPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, rbac, targets, authorization }
    }

    /// 管理端打开审计查询时，按操作者、动作类型、对象类型或结果等条件分页，在同一读取事务内复验后交付。
    ///
    /// 分页参数非法或排序字段不在白名单时返回 `ValidationError`；查询失败返回 `RepositoryError`。
    /// 返回 `items`、`total`、`page` 和 `page_size`。
    pub async fn audit_event_list(
        &self,
        params: &AuditEventListParams,
        actor: &AuditActor,
    ) -> Result<PageView<AuditEventView>> {
        let db = self.db.clone();
        let params = params.clone();
        ManagementAccess::new(self.authorization.clone(), actor.clone(), &access_control::AUDIT_EVENT_LIST)
            .verified_read_tx(&self.db, move |executor| {
                Box::pin(async move {
                    params.validate()?;
                    let filter = params.normalized()?;
                    let page = db.audit_events().search_audit_events(&filter, executor).await?;
                    let items = page.items.into_iter().map(AuditEventView::from).collect();
                    Ok(PageView {
                        items,
                        total: page.total,
                        page: filter.paging.page,
                        page_size: filter.paging.page_size,
                    })
                })
            })
            .await
    }

    // 写事务前构造与 `audit_log` 字段对齐的成功审计事件；动作未登记或字段不合规时拒绝。
    // 操作者快照取账号名称（无账号时用「系统操作人」）和账号类型；变更只留字段名，不留字段值。
    async fn build_audit_event(
        &self,
        actor: &AuditActor,
        action_type: &str,
        object_type: &str,
        object_id: Option<String>,
        changed_field_names: Vec<String>,
    ) -> Result<AuditEvent> {
        if !crate::entity::access_control::audit_actions::AUDIT_ACTIONS
            .iter()
            .any(|(code, _)| *code == action_type)
        {
            return Err(Error::ValidationError("审计写入动作未登记".into()));
        }

        let account = self.db.accounts().find_by_id(actor.id(), &mut NoTransaction).await?;
        let actor_label = account.map(|account| account.name).unwrap_or_else(|| "系统操作人".to_string());
        Ok(AuditEvent::new(
            AuditEventId::new(next_id()),
            AuditEventData {
                actor_id: actor.id().to_string(),
                actor_label,
                actor_role: actor.kind().as_str().to_string(),
                action_type: action_type.to_string(),
                object_type: object_type.to_string(),
                object_id,
                object_label: None,
                request_id: None,
                trace_id: None,
                result: AuditEventResult::Success,
                changed_field_names,
                safe_digest: None,
                source_ip: None,
                device_context: None,
            },
        )?)
    }
}
