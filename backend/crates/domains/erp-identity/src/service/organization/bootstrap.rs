//! 空组织树首次启动：建立默认部门并把无主属后台账号挂进去。
//!
//! 只在尚无任何组织节点时执行；已有组织树时不得改派，以免覆盖管理员配置。

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::entity::organization::OrgUnitKind;
use crate::entity::organization_change::{
    OrganizationChangeReceipt, OrganizationChangeRequest, OrganizationOperation, OrganizationState,
};
use crate::repository::OrganizationRepository;
use crate::repository::prelude::*;
use crate::{AccessControlExt, Result};

const HOME_DEPARTMENT: &str = "总部";
const BOOTSTRAP_ACTOR: &str = "system";
const BOOTSTRAP_REASON: &str = "初始化默认主属组织";
const BOOTSTRAP_RECEIPT_ID: &str = "system:org-home-bootstrap-v1";
const BOOTSTRAP_IDEMPOTENCY: &str = "org-home-bootstrap-v1";

/// 初始化空组织树时创建默认部门，并为当前有效后台账号建立主属关系。已有组织树或引导回执时保留现有配置，读取或写入错误原样返回。
pub(crate) async fn ensure_home_department(db: Database) -> Result<()> {
    let stored = db.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { persist_home_department(&stored, executor).await })
        })
        .await
}

// 沿启动事务读取组织与账号，按需保存默认部门、主属关系和引导回执，使首次配置可安全重试。
async fn persist_home_department(db: &Database, executor: &mut dyn Executor) -> Result<()> {
    let repository = OrganizationRepository::new(db);
    if repository.receipt(BOOTSTRAP_RECEIPT_ID, executor).await?.is_some() {
        return Ok(());
    }

    let before = repository.state(executor).await?;
    let accounts = db.accounts().list_by_kind(erp_core::AccountKind::Admin, executor).await?;
    let account_ids = accounts
        .into_iter()
        .filter(|account| account.is_active_backoffice())
        .map(|account| account.base.id)
        .collect::<Vec<_>>();
    let Some(after) = plan_home_department(&before, &account_ids, Instant::from_unix_secs(1))? else {
        return Ok(());
    };

    let mut receipt = OrganizationChangeReceipt {
        base: entity_core::BaseModel::new(BOOTSTRAP_RECEIPT_ID.to_string()),
        actor_id: BOOTSTRAP_ACTOR.into(),
        request: OrganizationChangeRequest {
            expected_version: before.version,
            idempotency_key: BOOTSTRAP_IDEMPOTENCY.into(),
            reason: BOOTSTRAP_REASON.into(),
            change: OrganizationOperation::CreateUnit {
                name: HOME_DEPARTMENT.into(),
                parent_id: None,
                kind: OrgUnitKind::Department,
            },
        },
        before,
        after,
        as_of: Instant::from_unix_secs(1),
    };
    repository.save(&mut receipt, executor).await?;
    Ok(())
}

// 根据当前组织事实和有效后台账号计算默认部门及成员变更。已有组织节点返回 `None`；组织名称或成员关系不合法时返回错误。
fn plan_home_department(
    state: &OrganizationState,
    account_ids: &[String],
    at: Instant,
) -> Result<Option<OrganizationState>> {
    if !state.units.is_empty() {
        return Ok(None);
    }

    let unit_id = "org-home".to_string();
    let mut next = state.changed(
        &OrganizationChangeRequest {
            expected_version: state.version,
            idempotency_key: BOOTSTRAP_IDEMPOTENCY.into(),
            reason: BOOTSTRAP_REASON.into(),
            change: OrganizationOperation::CreateUnit {
                name: HOME_DEPARTMENT.into(),
                parent_id: None,
                kind: OrgUnitKind::Department,
            },
        },
        &unit_id,
        BOOTSTRAP_ACTOR,
        at,
    )?;

    for (index, account_id) in account_ids.iter().enumerate() {
        next = next.changed(
            &OrganizationChangeRequest {
                expected_version: next.version,
                idempotency_key: format!("{BOOTSTRAP_IDEMPOTENCY}-{index}"),
                reason: BOOTSTRAP_REASON.into(),
                change: OrganizationOperation::TransferMember {
                    user_id: account_id.clone(),
                    org_unit_id: unit_id.clone(),
                },
            },
            &format!("org-home-member-{index}"),
            BOOTSTRAP_ACTOR,
            at,
        )?;
    }
    Ok(Some(next))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证首次引导生成默认部门，并为传入的有效后台账号建立主属关系。
    #[test]
    fn empty_tree_creates_home_department_and_assigns_accounts() {
        let planned = plan_home_department(
            &OrganizationState::default(),
            &["sales".into(), "admin".into()],
            Instant::from_unix_secs(1),
        )
        .unwrap()
        .expect("empty tree should bootstrap");
        assert_eq!(planned.units.len(), 1);
        assert_eq!(planned.units[0].name, HOME_DEPARTMENT);
        assert_eq!(planned.own_org("sales", Instant::from_unix_secs(1)).unwrap(), Some("org-home"));
        assert_eq!(planned.own_org("admin", Instant::from_unix_secs(1)).unwrap(), Some("org-home"));
        assert_eq!(planned.version, 3);
    }

    // 验证已有组织节点时跳过默认部门计划，保留管理员的组织配置。
    #[test]
    fn existing_tree_is_left_untouched() {
        let mut state = OrganizationState { version: 1, ..OrganizationState::default() };
        state.units.push(
            crate::entity::organization::OrgUnit::new(
                "sales-dept".into(),
                "销售部".into(),
                None,
                OrgUnitKind::Department,
                "admin".into(),
                "已有组织".into(),
            )
            .unwrap(),
        );
        assert!(
            plan_home_department(&state, &["sales".into()], Instant::from_unix_secs(1)).unwrap().is_none()
        );
    }
}
