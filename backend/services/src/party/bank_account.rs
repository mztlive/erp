//! 域 D07 `party_bank_account` 服务编排。
//!
//! 银行账户按「有效期事实追加/结束」维护（W03：新增与修改仅财务）；
//! 账号是低熵敏感值（§4.5.5）：实体只保存带密钥 HMAC 指纹与密文，查询与
//! 重复校验只能使用 keyed HMAC（§6.2）。原地更新只允许切换启停状态、结束
//! 有效期与调整默认标记；同一主体同一时点最多一个默认有效账户（跨行约束，
//! 事务内校验，§6.2）。

use database::{AccessControlExt, NoTransaction, PartyExt, Transactional};
use entities::field_update::FieldUpdate;
use entities::party::{
    EffectiveRecordStatus, PartyBankAccount, PartyBankAccountData, PartyBankAccountId,
    PartyBankAccountUpdate, PartyId,
};
use id_generator::next_id;
use mongodb::Database;
use std::sync::Arc;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, CreatePartyBankAccountRequest, PageView, PartyBankAccountListParams,
    PartyBankAccountView, SortDir, UpdatePartyBankAccountRequest, PARTY_BANK_ACCOUNT_SORT_FIELDS,
};
use super::sensitive::SensitiveDataCodec;
use super::{clear_default_marks, page_or_default, page_size_or_default};

/// 银行账户列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyBankAccountFilter = <mongodb::Database as PartyExt>::PartyBankAccountFilter;

/// 银行账户服务。
pub struct PartyBankAccountService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
}

impl PartyBankAccountService {
    /// 创建银行账户服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self { db, sensitive_data }
    }

    /// 分页查询银行账户列表（投影查询，敏感字段不进投影）。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn party_bank_account_list(
        &self,
        party_id: &str,
        params: &PartyBankAccountListParams,
    ) -> Result<PageView<PartyBankAccountView>> {
        params.validate()?;
        super::ensure_outside_supplier_profile(&self.db, &PartyId::new(party_id)).await?;
        let (sort_by, sort_dir) =
            normalize_sort(&params.sort_by, &params.sort_dir, PARTY_BANK_ACCOUNT_SORT_FIELDS)?;
        let filter = PartyBankAccountFilter {
            party_id: Some(PartyId::new(party_id)),
            status: params.status,
            is_default: params.is_default,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .party_bank_accounts()
            .search_party_bank_accounts(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyBankAccountView {
                id: row.id,
                bank_account_no: row.bank_account_no,
                party_id: row.party_id.to_string(),
                account_name: row.account_name,
                bank_name: row.bank_name,
                account_number_masked: super::dto::masked_last4(&row.account_number_last4),
                bank_branch_name: row.bank_branch_name,
                valid_from: row.valid_from,
                valid_to: row.valid_to,
                is_default: row.is_default,
                status: row.status,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建银行账户（跨行事务：默认账户唯一 + 新建 + 审计原子写入）。
    ///
    /// 账号重复由 `(party_id, account_number_query_hmac)` 唯一索引拦截
    /// （§6.2：查询和重复校验只能使用 keyed HMAC），重复提交返回 409。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `req` - 创建请求（含账号明文）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建银行账户的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ConflictError` - 账号与既有账户重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_party_bank_account(
        &self,
        party_id: &str,
        req: CreatePartyBankAccountRequest,
        actor: &AuditActor,
    ) -> Result<PartyBankAccountView> {
        req.validate()?;
        self.ensure_party_exists(party_id).await?;
        super::ensure_outside_supplier_profile(&self.db, &PartyId::new(party_id)).await?;
        let account_number = req.account_number.clone();
        let mut account = PartyBankAccount::new(
            PartyBankAccountId::new(next_id()),
            PartyBankAccountData {
                bank_account_no: req.bank_account_no,
                party_id: PartyId::new(party_id),
                account_name: req.account_name,
                bank_name: req.bank_name,
                bank_branch_name: req.bank_branch_name,
                account_number: req.account_number,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                is_default: req.is_default,
                status: req.status.unwrap_or(EffectiveRecordStatus::Active),
            },
            self.sensitive_data.fingerprint_key(),
            actor.id(),
        )?;
        account.account_number_ciphertext = self.sensitive_data.encrypt(&account_number)?;
        let audit = actor.clone().resource_log(
            "party_bank_account.create",
            "party_bank_account",
            account.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let account_for_tx = account.clone();
        let party_id_for_tx = account.party_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if account_for_tx.is_default {
                        clear_default_marks!(db, party_bank_accounts, party_id_for_tx, None, session);
                    }
                    db.party_bank_accounts().create(&account_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(account.into())
    }

    /// 更新银行账户（仅生命周期字段；默认标记跨行事务）。
    ///
    /// # 参数
    /// * `id` - 银行账户 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后银行账户的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 银行账户不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_party_bank_account(
        &self,
        id: &str,
        req: UpdatePartyBankAccountRequest,
        actor: &AuditActor,
    ) -> Result<PartyBankAccountView> {
        req.validate()?;
        let mut account = self
            .db
            .party_bank_accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
        super::ensure_outside_supplier_profile(&self.db, &account.party_id).await?;
        if account.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        account.update(
            PartyBankAccountUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "party_bank_account.update",
            "party_bank_account",
            account.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut account_for_tx = account.clone();
        let party_id_for_tx = account.party_id.clone();
        let exclude_id = account.base.id.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if account_for_tx.is_default {
                        clear_default_marks!(
                            db,
                            party_bank_accounts,
                            party_id_for_tx,
                            Some(&exclude_id),
                            session
                        );
                    }
                    db.party_bank_accounts()
                        .update(&mut account_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PartyBankAccount, crate::errors::Error>(account_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验主体存在。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    ///
    /// # 返回
    /// 主体存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    async fn ensure_party_exists(&self, party_id: &str) -> Result<()> {
        self.db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("主体不存在".to_string()))?;
        Ok(())
    }
}
