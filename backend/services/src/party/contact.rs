//! 域 D07 `party_contact` 服务编排。
//!
//! 联系人按「有效期事实追加」维护（W03），内容变更不原地修改；
//! 手机号是低熵敏感值（§4.5.5）：实体只保存带密钥 HMAC 指纹与密文，
//! 指纹密钥见 `super::sensitive`。原地更新只允许切换启停状态、结束有效期
//! 与调整默认标记；同一主体默认联系人唯一（跨行约束，事务内校验，§6.2）。

use database::{AccessControlExt, NoTransaction, PartyExt, Transactional};
use entities::field_update::FieldUpdate;
use entities::party::{
    EffectiveRecordStatus, PartyContact, PartyContactData, PartyContactId, PartyContactUpdate, PartyId,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, CreatePartyContactRequest, PageView, PartyContactListParams, PartyContactView, SortDir,
    UpdatePartyContactRequest, PARTY_CONTACT_SORT_FIELDS,
};
use super::sensitive::FINGERPRINT_KEY;
use super::{clear_default_marks, normalized_text, page_or_default, page_size_or_default};

/// 联系人列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyContactFilter = <mongodb::Database as PartyExt>::PartyContactFilter;

/// 联系人服务。
pub struct PartyContactService {
    db: Database,
}

impl PartyContactService {
    /// 创建联系人服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询联系人列表（投影查询，敏感字段不进投影）。
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
    pub async fn party_contact_list(
        &self,
        party_id: &str,
        params: &PartyContactListParams,
    ) -> Result<PageView<PartyContactView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            normalize_sort(&params.sort_by, &params.sort_dir, PARTY_CONTACT_SORT_FIELDS)?;
        let filter = PartyContactFilter {
            party_id: Some(PartyId::new(party_id)),
            keyword: normalized_text(params.keyword.as_deref()),
            mobile_query_hmac: None,
            status: params.status,
            is_default: params.is_default,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .party_contacts()
            .search_party_contacts(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyContactView {
                id: row.id,
                party_id: row.party_id.to_string(),
                contact_name: row.contact_name,
                title: row.title,
                telephone: row.telephone,
                email: row.email,
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

    /// 创建联系人（跨行事务：默认联系人唯一 + 新建 + 审计原子写入）。
    ///
    /// `is_default = true` 时在同一事务内清除该主体其他联系人的默认标记
    /// （§6.2：同一主体默认联系人唯一）。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `req` - 创建请求（含手机号明文）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建联系人的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_party_contact(
        &self,
        party_id: &str,
        req: CreatePartyContactRequest,
        actor: &AuditActor,
    ) -> Result<PartyContactView> {
        req.validate()?;
        self.ensure_party_exists(party_id).await?;
        let contact = PartyContact::new(
            PartyContactId::new(next_id()),
            PartyContactData {
                party_id: PartyId::new(party_id),
                contact_name: req.contact_name,
                title: req.title,
                mobile: req.mobile,
                telephone: req.telephone,
                email: req.email,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                is_default: req.is_default,
                status: req.status.unwrap_or(EffectiveRecordStatus::Active),
            },
            FINGERPRINT_KEY,
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("party_contact.create", "party_contact", contact.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let contact_for_tx = contact.clone();
        let party_id_for_tx = contact.party_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if contact_for_tx.is_default {
                        clear_default_marks!(db, party_contacts, party_id_for_tx, None, session);
                    }
                    db.party_contacts().create(&contact_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(contact.into())
    }

    /// 更新联系人（仅生命周期字段；默认标记跨行事务）。
    ///
    /// # 参数
    /// * `id` - 联系人 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后联系人的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 联系人不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_party_contact(
        &self,
        id: &str,
        req: UpdatePartyContactRequest,
        actor: &AuditActor,
    ) -> Result<PartyContactView> {
        req.validate()?;
        let mut contact = self
            .db
            .party_contacts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
        if contact.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        contact.update(
            PartyContactUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("party_contact.update", "party_contact", contact.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut contact_for_tx = contact.clone();
        let party_id_for_tx = contact.party_id.clone();
        let exclude_id = contact.base.id.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if contact_for_tx.is_default {
                        clear_default_marks!(db, party_contacts, party_id_for_tx, Some(&exclude_id), session);
                    }
                    db.party_contacts().update(&mut contact_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PartyContact, crate::errors::Error>(contact_for_tx)
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
