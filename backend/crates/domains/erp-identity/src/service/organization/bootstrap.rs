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

/// 组织树为空时创建默认部门，并为当前有效后台账号写入主属关系。
///
/// # 参数
/// * `db` - 身份与组织集合所在数据库
///
/// # 返回
/// 已有组织、已有引导回执或引导完成后返回 `Ok`。
///
/// # 错误
/// 账号读取、组织校验或事务写入失败时拒绝。
///
/// # 关键业务约束
/// 不得在已有组织树上自动调岗；不得把系统账号映射成销售人员。
pub(crate) async fn ensure_home_department(db: Database) -> Result<()> {
    let stored = db.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { persist_home_department(&stored, executor).await })
        })
        .await
}

/// 在调用方事务内读取组织事实并按需持久化默认主属部门。
///
/// # 参数
/// * `db` - 身份数据库
/// * `executor` - 调用方执行器
///
/// # 返回
/// 已引导、无需引导或写入完成后返回 `Ok`。
///
/// # 错误
/// 组织校验或写入失败时拒绝。
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

/// 计算空组织树的默认部门与主属关系；已有节点时不改派。
///
/// # 参数
/// * `state` - 当前组织事实
/// * `account_ids` - 需要写入主属关系的有效后台账号
/// * `at` - 主属关系生效时点
///
/// # 返回
/// 无需引导返回 `None`；需要引导时返回变更后的组织事实。
///
/// # 错误
/// 组织名称或主属关系校验失败时拒绝。
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
