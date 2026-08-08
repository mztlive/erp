//! 域 D07 `party_address` 服务编排。
//!
//! 地址按「有效期事实追加」维护（W03），内容变更不原地修改；履约地址是
//! 低熵敏感值（§4.5.5）：实体只保存带密钥 HMAC 指纹与密文。原地更新只
//! 允许切换启停状态、结束有效期与调整默认标记；同一主体默认地址唯一
//! （跨行约束，事务内校验，§6.2）。

use database::{AccessControlExt, NoTransaction, PartyExt, Transactional};
use entities::field_update::FieldUpdate;
use entities::party::{
    EffectiveRecordStatus, PartyAddress, PartyAddressData, PartyAddressId, PartyAddressUpdate, PartyId,
};
use id_generator::next_id;
use mongodb::Database;
use std::sync::Arc;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, CreatePartyAddressRequest, PageView, PartyAddressListParams, PartyAddressView, SortDir,
    UpdatePartyAddressRequest, PARTY_ADDRESS_SORT_FIELDS,
};
use super::sensitive::SensitiveDataCodec;
use super::{clear_default_marks, page_or_default, page_size_or_default};

/// 地址列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyAddressFilter = <mongodb::Database as PartyExt>::PartyAddressFilter;

/// 地址服务。
pub struct PartyAddressService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
}

impl PartyAddressService {
    /// 创建地址服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self { db, sensitive_data }
    }

    /// 分页查询地址列表（投影查询，敏感字段不进投影）。
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
    pub async fn party_address_list(
        &self,
        party_id: &str,
        params: &PartyAddressListParams,
    ) -> Result<PageView<PartyAddressView>> {
        params.validate()?;
        super::ensure_outside_supplier_profile(&self.db, &PartyId::new(party_id)).await?;
        let (sort_by, sort_dir) =
            normalize_sort(&params.sort_by, &params.sort_dir, PARTY_ADDRESS_SORT_FIELDS)?;
        let filter = PartyAddressFilter {
            party_id: Some(PartyId::new(party_id)),
            address_type: params.address_type,
            status: params.status,
            is_default: params.is_default,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .party_addresses()
            .search_party_addresses(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyAddressView {
                id: row.id,
                party_id: row.party_id.to_string(),
                address_type: row.address_type,
                contact_name: row.contact_name,
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

    /// 创建地址（跨行事务：默认地址唯一 + 新建 + 审计原子写入）。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `req` - 创建请求（含地址明文）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建地址的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_party_address(
        &self,
        party_id: &str,
        req: CreatePartyAddressRequest,
        actor: &AuditActor,
    ) -> Result<PartyAddressView> {
        req.validate()?;
        self.ensure_party_exists(party_id).await?;
        super::ensure_outside_supplier_profile(&self.db, &PartyId::new(party_id)).await?;
        let address_plaintext = req.address.clone();
        let mut address = PartyAddress::new(
            PartyAddressId::new(next_id()),
            PartyAddressData {
                party_id: PartyId::new(party_id),
                address_type: req.address_type,
                contact_name: req.contact_name,
                address: req.address,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                is_default: req.is_default,
                status: req.status.unwrap_or(EffectiveRecordStatus::Active),
            },
            self.sensitive_data.fingerprint_key(),
            actor.id(),
        )?;
        address.address_ciphertext = self.sensitive_data.encrypt(&address_plaintext)?;
        let audit =
            actor
                .clone()
                .resource_log("party_address.create", "party_address", address.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let address_for_tx = address.clone();
        let party_id_for_tx = address.party_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if address_for_tx.is_default {
                        clear_default_marks!(db, party_addresses, party_id_for_tx, None, session);
                    }
                    db.party_addresses().create(&address_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(address.into())
    }

    /// 更新地址（仅生命周期字段；默认标记跨行事务）。
    ///
    /// # 参数
    /// * `id` - 地址 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后地址的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 地址不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_party_address(
        &self,
        id: &str,
        req: UpdatePartyAddressRequest,
        actor: &AuditActor,
    ) -> Result<PartyAddressView> {
        req.validate()?;
        let mut address = self
            .db
            .party_addresses()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
        super::ensure_outside_supplier_profile(&self.db, &address.party_id).await?;
        if address.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        address.update(
            PartyAddressUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("party_address.update", "party_address", address.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut address_for_tx = address.clone();
        let party_id_for_tx = address.party_id.clone();
        let exclude_id = address.base.id.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if address_for_tx.is_default {
                        clear_default_marks!(
                            db,
                            party_addresses,
                            party_id_for_tx,
                            Some(&exclude_id),
                            session
                        );
                    }
                    db.party_addresses().update(&mut address_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PartyAddress, crate::errors::Error>(address_for_tx)
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
