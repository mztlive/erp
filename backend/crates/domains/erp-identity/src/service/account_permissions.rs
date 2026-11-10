//! 有效后台账号的功能权限候选与写入前资格重验。

use std::future::Future;

use application_core::AuditActor;
use erp_core::AccountKind;
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::AccountPermissionCandidate;
use crate::entity::{AccountCore, Permission};
use crate::repository::prelude::*;
use crate::service::iam::{SharedRbacService, subject};
use crate::{AccessControlExt, Error, Result};

/// 查询有效且具备指定功能权限的内部账号候选。
///
/// # 参数
/// * `db` - 身份数据库。
/// * `rbac` - 当前共享权限服务。
/// * `excluded_id` - 需要排除的当前责任人。
/// * `permission` - 调用方固定的功能权限。
/// * `executor` - 调用方的数据执行器。
///
/// # 返回
/// 返回按姓名、登录账号、稳定 ID 排序的候选。
///
/// # 错误
/// 账号读取、权限解析或 RBAC 判定失败时返回原错误，不降级为空候选。
///
/// # 关键业务约束
/// 本方法只证明账号资格，不授予业务对象访问权；调用方必须先校验对象访问。
pub async fn permission_candidates(
    db: &Database,
    rbac: &SharedRbacService,
    excluded_id: &str,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<Vec<AccountPermissionCandidate>> {
    let accounts = db.accounts().list_by_kind(AccountKind::Admin, executor).await?;
    candidates_from_accounts(accounts, excluded_id, permission, |identity, permission| async move {
        rbac.enforce(&identity, &permission).await
    })
    .await
}

/// 在调用方执行器内重新读取目标账号并核验当前功能权限。
///
/// # 参数
/// * `db` - 身份数据库。
/// * `rbac` - 当前共享权限服务。
/// * `user_id` - 目标账号 ID。
/// * `permission` - 调用方固定的功能权限。
/// * `executor` - 与业务写入相同的数据执行器。
///
/// # 返回
/// 账号存在、为有效后台账号且具备权限时返回 `true`，否则返回 `false`。
///
/// # 错误
/// 仓储或 RBAC 失败时返回原错误；调用方负责自己的资格拒绝文案。
pub async fn account_has_permission(
    db: &Database,
    rbac: &SharedRbacService,
    user_id: &str,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let account = db.accounts().find_by_id(user_id, executor).await?;
    account_is_allowed(account.as_ref(), permission, |identity, permission| async move {
        rbac.enforce(&identity, &permission).await
    })
    .await
}

/// 在调用方事务内证明有效账号由同一有效角色获得全部所需权限。
///
/// # 参数
/// * `db` - 身份数据库。
/// * `rbac` - 当前共享权限服务。
/// * `actor` - 当前后台操作身份。
/// * `required` - 消费方确定的完整权限集合。
/// * `executor` - 与业务写入相同的执行器。
///
/// # 返回
/// 返回已经过事务内重验的权限策略版本。
///
/// # 错误
/// 账号失效、角色停用、任一权限缺失或策略版本变化时返回原身份错误。
pub async fn ensure_actor_permissions(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    required: &[Permission],
    executor: &mut dyn Executor,
) -> Result<u64> {
    let account = db.accounts().find_by_id(actor.id(), executor).await?;
    with_actor_permissions(account, actor, required, |required| {
        ensure_role_permissions(db, rbac, actor, required, executor)
    })
    .await
}

async fn with_actor_permissions<'a, F, Fut>(
    account: Option<AccountCore>,
    actor: &AuditActor,
    permissions: &'a [Permission],
    authorize: F,
) -> Result<u64>
where
    F: FnOnce(&'a [Permission]) -> Fut,
    Fut: Future<Output = Result<u64>>,
{
    account
        .filter(|account| account.is_active_backoffice() && account.kind == actor.kind())
        .ok_or_else(|| Error::Forbidden("账号已失效".into()))?;
    authorize(permissions).await
}

async fn ensure_role_permissions(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    required: &[Permission],
    executor: &mut dyn Executor,
) -> Result<u64> {
    let snapshot = rbac.role_permission_snapshot(actor.kind(), actor.id(), required).await?;
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    let roles = snapshot.granting_role_ids_for_all(required);
    if db.roles().enabled_roles(&roles, executor).await?.is_empty() {
        return Err(Error::Forbidden(snapshot.denial_message(required)));
    }
    Ok(snapshot.policy_revision())
}

async fn candidates_from_accounts<F, Fut>(
    accounts: Vec<AccountCore>,
    excluded_id: &str,
    permission: &Permission,
    mut check: F,
) -> Result<Vec<AccountPermissionCandidate>>
where
    F: FnMut(String, Permission) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    let mut candidates = Vec::new();
    for account in accounts {
        if account.base.id == excluded_id {
            continue;
        }
        if account_is_allowed(Some(&account), permission, &mut check).await? {
            candidates.push(AccountPermissionCandidate::from_account(&account));
        }
    }
    candidates.sort();
    Ok(candidates)
}

async fn account_is_allowed<F, Fut>(
    account: Option<&AccountCore>,
    permission: &Permission,
    check: F,
) -> Result<bool>
where
    F: FnOnce(String, Permission) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    let Some(account) = account.filter(|account| account.is_active_backoffice()) else {
        return Ok(false);
    };
    check(subject(account.kind, &account.base.id), permission.clone()).await
}

#[cfg(test)]
mod tests {
    use std::future::{Ready, ready};
    use std::mem::discriminant;
    use std::sync::{Arc, Mutex};

    use entity_core::BaseModel;

    use super::*;
    use crate::Error;
    use crate::entity::{AccountStatus, LoginAccount, Secret};

    fn account(id: &str, name: &str, login: &str) -> AccountCore {
        let mut secret = Secret::default();
        secret.change_account(LoginAccount::new(login).unwrap());
        let mut base = BaseModel::fake();
        base.id = id.into();
        AccountCore {
            base,
            secret,
            name: name.into(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        }
    }

    #[tokio::test]
    async fn account_failures_precede_authorization_and_permission_failures_propagate() {
        let actor = AuditActor::new("actor".into(), "buyer".into(), AccountKind::Admin);
        let required = [Permission::parse("procurement_inquiry:detail").unwrap()];
        let mut suspended = account("actor", "采购", "buyer");
        suspended.status = AccountStatus::Suspended;
        let mut supplier = account("actor", "门户", "supplier");
        supplier.kind = AccountKind::Supplier;
        for value in [None, Some(suspended), Some(supplier)] {
            let error = with_actor_permissions(value, &actor, &required, |_| -> Ready<Result<u64>> {
                panic!("无效账号不得读取授权")
            })
            .await
            .unwrap_err();
            assert!(matches!(error, Error::Forbidden(_)));
        }
        for failure in [
            Error::Rbac("unavailable".into()),
            Error::ConflictError("changed".into()),
            Error::Forbidden("denied".into()),
        ] {
            let expected = discriminant(&failure);
            let error =
                with_actor_permissions(Some(account("actor", "采购", "buyer")), &actor, &required, |_| {
                    ready(Err(failure))
                })
                .await
                .unwrap_err();
            assert_eq!(discriminant(&error), expected);
        }
    }

    #[tokio::test]
    async fn typed_permissions_are_forwarded_without_reparsing_or_replacing_the_slice() {
        let actor = AuditActor::new("actor".into(), "buyer".into(), AccountKind::Admin);
        let required = [
            Permission::parse("product:update").unwrap(),
            Permission::parse("supplier_offering:update").unwrap(),
        ];
        let revision = with_actor_permissions(
            Some(account("actor", "采购", "buyer")),
            &actor,
            &required,
            |permissions| {
                assert_eq!(permissions.as_ptr(), required.as_ptr());
                assert_eq!(permissions, required.as_slice());
                ready(Ok(51))
            },
        )
        .await
        .unwrap();
        assert_eq!(revision, 51);
    }

    #[tokio::test]
    async fn candidates_exclude_current_inactive_and_unqualified_then_sort_all_ties() {
        let current = account("current", "当前维护人", "current");
        let mut inactive = account("inactive", "停用", "inactive");
        inactive.status = AccountStatus::Suspended;
        let mut supplier = account("supplier", "门户", "supplier");
        supplier.kind = AccountKind::Supplier;
        let accounts = vec![
            current,
            inactive,
            supplier,
            account("id-3", "B", "login-a"),
            account("id-2", "A", "login-a"),
            account("id-1", "A", "login-a"),
            account("id-4", "A", "login-b"),
            account("denied", "拒绝", "denied"),
        ];
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let candidates = candidates_from_accounts(
            accounts,
            "current",
            &Permission::parse("product:update").unwrap(),
            move |identity, permission| {
                recorded.lock().unwrap().push((identity.clone(), permission.to_string()));
                std::future::ready(Ok(identity != "user:admin:denied"))
            },
        )
        .await
        .unwrap();
        assert_eq!(
            candidates.iter().map(|candidate| candidate.user_id.as_str()).collect::<Vec<_>>(),
            vec!["id-1", "id-2", "id-4", "id-3"]
        );
        assert_eq!(candidates[0].account, "login-a");
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[
                ("user:admin:id-3".to_string(), "product:update".to_string()),
                ("user:admin:id-2".to_string(), "product:update".to_string()),
                ("user:admin:id-1".to_string(), "product:update".to_string()),
                ("user:admin:id-4".to_string(), "product:update".to_string()),
                ("user:admin:denied".to_string(), "product:update".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn target_qualification_uses_canonical_subject_and_keeps_requested_permission() {
        for key in ["product:update", "supplier_offering:update"] {
            let target = account("target-1", "维护人", "maintainer");
            let allowed = account_is_allowed(
                Some(&target),
                &Permission::parse(key).unwrap(),
                |identity, permission| {
                    assert_eq!(identity, "user:admin:target-1");
                    assert_eq!(permission.to_string(), key);
                    std::future::ready(Ok(true))
                },
            )
            .await
            .unwrap();
            assert!(allowed);
        }
    }

    #[tokio::test]
    async fn missing_or_inactive_target_does_not_query_rbac() {
        let mut inactive = account("inactive", "停用", "inactive");
        inactive.status = AccountStatus::Archived;
        let mut supplier = account("supplier", "门户", "supplier");
        supplier.kind = AccountKind::Supplier;
        for target in [None, Some(&inactive), Some(&supplier)] {
            assert!(
                !account_is_allowed(
                    target,
                    &Permission::parse("product:update").unwrap(),
                    |_, _| -> std::future::Ready<Result<bool>> { panic!("不合格账号不得查询 RBAC") }
                )
                .await
                .unwrap()
            );
        }
    }

    #[tokio::test]
    async fn rbac_failure_stops_candidate_collection_without_returning_partial_results() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let result = candidates_from_accounts(
            vec![
                account("first", "甲", "first"),
                account("failed", "乙", "failed"),
                account("last", "丙", "last"),
            ],
            "current",
            &Permission::parse("supplier_offering:update").unwrap(),
            move |identity, _| {
                recorded.lock().unwrap().push(identity.clone());
                std::future::ready(if identity == "user:admin:failed" {
                    Err(Error::Rbac("policy unavailable".into()))
                } else {
                    Ok(true)
                })
            },
        )
        .await;
        assert!(matches!(result, Err(Error::Rbac(message)) if message == "policy unavailable"));
        assert_eq!(calls.lock().unwrap().as_slice(), &["user:admin:first", "user:admin:failed"]);
    }

    #[tokio::test]
    async fn target_permission_denial_remains_false_and_rbac_failure_remains_error() {
        let target = account("target", "维护人", "target");
        assert!(
            !account_is_allowed(Some(&target), &Permission::parse("product:update").unwrap(), |_, _| {
                std::future::ready(Ok(false))
            })
            .await
            .unwrap()
        );
        let error =
            account_is_allowed(Some(&target), &Permission::parse("product:update").unwrap(), |_, _| {
                std::future::ready(Err(Error::Rbac("policy unavailable".into())))
            })
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Rbac(message) if message == "policy unavailable"));
    }
}
