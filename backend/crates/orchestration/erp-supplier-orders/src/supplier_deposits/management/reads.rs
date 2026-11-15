//! 单账户范围拒绝在名称、余额和资金证据投影之前完成。
use std::sync::Arc;

use application_core::AuditActor;
use erp_finance::entity::supplier_deposit::SupplierDepositAccount;
use erp_finance::repository::supplier_deposit_query::account_page;
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_identity::AuthorizationService;
use erp_permissions::supplier_deposit;
use erp_read_models::finance::supplier_deposits::{
    DepositAccountDetail, DepositAccountRow, account_detail, account_row,
};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::access::{account_access, audit, identity_access};
use super::{DepositAccountList, DepositAccountQuery, OpenDepositAccount, SupplierDepositManagementProcess};
use crate::{Error, Result};

impl SupplierDepositManagementProcess {
    /// 有界读取逐账户获得主体及组织许可的列表。
    /// # 参数
    /// * `query` - 仅收窄范围的供应商、主体和分页筛选。
    /// * `actor` - 当前认证财务人员。
    /// # 返回
    /// 返回安全业务名称与账户状态；不暴露其他账户总数。
    /// # 错误
    /// 参数或权限解析非法、数据库失败时拒绝。
    pub async fn list(
        &self,
        query: DepositAccountQuery,
        actor: &AuditActor,
    ) -> Result<DepositAccountList<DepositAccountRow>> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        let size = u64::from(query.page_size.unwrap_or(20).clamp(1, 100));
        let skip = query
            .page
            .unwrap_or(1)
            .max(1)
            .checked_sub(1)
            .and_then(|page| page.checked_mul(size))
            .ok_or_else(|| Error::ValidationError("分页范围过大".into()))?;
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    list_accounts(&db, &authorization, query, &actor, skip, size, executor).await
                })
            })
            .await
    }

    /// 读取已获财务范围许可的账户证据。
    /// # 参数
    /// * `id` - 服务端资金账户身份。
    /// * `actor` - 当前认证账号。
    /// # 返回
    /// 返回账户、资金分类及脱敏收款账户候选。
    /// # 错误
    /// 越范围、缺归属或数据库失败时拒绝。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<DepositAccountDetail> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let id = id.to_string();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let account =
                        account_access(&db, &authorization, &id, &actor, "detail", executor).await?;
                    Ok(account_detail(&db, account, executor).await?)
                })
            })
            .await
    }

    /// 新建真实归属的空余额账户。
    /// # 参数
    /// * `command` - 明确账户归属和稳定操作身份。
    /// * `actor` - 具备开户范围权限的财务人员。
    /// # 返回
    /// 返回账户；任何开账户操作都不导入远端余额。
    /// # 错误
    /// 越主体、越组织或重复身份冲突时拒绝。
    pub async fn open(
        &self,
        command: OpenDepositAccount,
        actor: &AuditActor,
    ) -> Result<SupplierDepositAccount> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    identity_access(&db, &authorization, &command.identity, &actor, "create", executor)
                        .await?;
                    let account = SupplierDepositService::new(db.clone())
                        .open_account(&command.operation_key, command.identity, executor)
                        .await?;
                    audit(&db, &actor, "create", &account.base.id, executor).await?;
                    Ok(account)
                })
            })
            .await
    }
}

async fn list_accounts(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    query: DepositAccountQuery,
    actor: &AuditActor,
    skip: u64,
    size: u64,
    executor: &mut dyn Executor,
) -> Result<DepositAccountList<DepositAccountRow>> {
    let compiled = authorization.query(actor, &supplier_deposit::LIST, executor).await?;
    let mut candidates = account_page(
        db,
        query.supplier_id.as_deref(),
        query.payer_party_id.as_deref(),
        skip,
        u32::try_from(size).map_err(|_| Error::ValidationError("分页范围无效".into()))?,
        &compiled,
        executor,
    )
    .await?;
    let has_more = candidates.len()
        > usize::try_from(size).map_err(|_| Error::ValidationError("分页范围无效".into()))?;
    if has_more {
        candidates.pop();
    }
    let mut items = Vec::new();
    for candidate in candidates {
        items.push(account_row(db, candidate, executor).await?);
    }
    authorization.validate(&compiled, executor).await?;
    Ok(DepositAccountList { items, has_more })
}
