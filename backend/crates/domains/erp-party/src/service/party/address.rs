//! 域 D07 `party_address` 服务编排。
//!
//! 地址按「有效期事实追加」维护（W03），内容变更不原地修改；履约地址是
//! 低熵敏感值（§4.5.5）：实体只保存带密钥 HMAC 指纹与密文。原地更新只
//! 允许切换启停状态、结束有效期与调整默认标记；同一主体默认地址唯一
//! （跨行约束，事务内校验，§6.2）。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::field_update::FieldUpdate;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::party;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::sensitive::SensitiveDataCodec;
use crate::dto::party::{
    CreatePartyAddressRequest, PARTY_ADDRESS_SORT_FIELDS, PageView, PartyAddressListParams, PartyAddressView,
    SortDir, UpdatePartyAddressRequest, normalize_paging,
};
use crate::entity::party::{
    EffectiveRecordStatus, PartyAddress, PartyAddressData, PartyAddressId, PartyAddressUpdate, PartyId,
};
use crate::error::{Error, Result};
use crate::ports::{PartyAuditPort, SupplierRolePort};
use crate::repository::prelude::*;
use crate::repository::{PartyAddressFilter, PartyExt};

/// 地址服务。
#[derive(Clone)]
pub struct PartyAddressService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    audit: Arc<dyn PartyAuditPort>,
    supplier_roles: Arc<dyn SupplierRolePort>,
    authorization: Arc<AuthorizationService>,
}

impl PartyAddressService {
    /// 创建地址服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `sensitive_data` - 敏感数据编解码器
    /// * `audit` - 审计写入端口
    /// * `supplier_roles` - 供应商角色事实端口
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        sensitive_data: Arc<SensitiveDataCodec>,
        audit: Arc<dyn PartyAuditPort>,
        supplier_roles: Arc<dyn SupplierRolePort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, sensitive_data, audit, supplier_roles, authorization }
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
        actor: &AuditActor,
        party_id: &str,
        params: &PartyAddressListParams,
    ) -> Result<PageView<PartyAddressView>> {
        let service = self.clone();
        let actor = actor.clone();
        let party_id = party_id.to_owned();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.party_address_list_snapshot(&actor, &party_id, &params, executor).await
                })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn party_address_list_snapshot(
        &self,
        actor: &AuditActor,
        party_id: &str,
        params: &PartyAddressListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<PartyAddressView>> {
        self.authorization.require(actor, &party::ADDRESS_LIST, AccessTarget::Operation, executor).await?;
        params.validate()?;
        super::ensure_outside_supplier_profile(
            self.supplier_roles.as_ref(),
            &PartyId::new(party_id),
            executor,
        )
        .await?;
        let paging = normalize_paging(
            &params.sort_by,
            &params.sort_dir,
            params.page,
            params.page_size,
            PARTY_ADDRESS_SORT_FIELDS,
        )?;
        let filter = PartyAddressFilter {
            party_id: Some(PartyId::new(party_id)),
            address_type: params.address_type,
            status: params.status,
            is_default: params.is_default,
            page: paging.page,
            page_size: paging.page_size,
            sort_by: Some(paging.sort_by.to_string()),
            sort_ascending: matches!(paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.party_addresses().search_party_addresses(&filter, executor).await?;
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

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
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
        let service = self.clone();
        let actor = actor.clone();
        let party_id = party_id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.create_stage(&party_id, req, &actor, executor).await })
            })
            .await
    }

    /// 仅使用已验证输入构造本域事实，不读取数据库。
    fn prepare_record(
        &self,
        party_id: &str,
        req: CreatePartyAddressRequest,
        actor: &AuditActor,
    ) -> Result<PartyAddress> {
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
        Ok(address)
    }

    /// 在授权快照中核验主体和供应商边界，再原子写入默认标记、事实与审计。
    async fn create_stage(
        &self,
        party_id: &str,
        req: CreatePartyAddressRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PartyAddressView> {
        self.authorization.require(actor, &party::ADDRESS_CREATE, AccessTarget::Operation, executor).await?;
        super::ensure_new_fact_guards(&self.db, self.supplier_roles.as_ref(), party_id, executor).await?;
        let address = self.prepare_record(party_id, req, actor)?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party_address.create",
            "party_address",
            address.base.id.clone(),
        )?;
        if address.is_default {
            self.db.party_addresses().clear_other_default_marks(&address.party_id, None, executor).await?;
        }
        self.db.party_addresses().create(&address, executor).await?;
        self.audit.persist(&audit, executor).await?;
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
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.update_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    /// 先验证功能资格，再从原事务读取版本及完整事实，修改与审计使用同一执行器。
    async fn update_stage(
        &self,
        id: &str,
        req: UpdatePartyAddressRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PartyAddressView> {
        self.authorization.require(actor, &party::ADDRESS_UPDATE, AccessTarget::Operation, executor).await?;
        let mut address = self
            .db
            .party_addresses()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("地址不存在".into()))?;
        super::ensure_outside_supplier_profile(self.supplier_roles.as_ref(), &address.party_id, executor)
            .await?;
        super::map_version_conflict(address.ensure_version(req.version))?;
        address.update(
            PartyAddressUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party_address.update",
            "party_address",
            address.base.id.clone(),
        )?;
        if address.is_default {
            self.db
                .party_addresses()
                .clear_other_default_marks(&address.party_id, Some(address.base.id.as_str()), executor)
                .await?;
        }
        self.db.party_addresses().update(&mut address, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(address.into())
    }
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, FailClosedSupplierRolePort};

    async fn service() -> PartyAddressService {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("party_address_authorization");
        PartyAddressService::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"party-test-secret")),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedSupplierRolePort),
            unavailable_authorization(),
        )
    }

    #[tokio::test]
    async fn update_authorizes_before_existence_version_or_supplier_reads() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for version in [1, 999] {
            let req = serde_json::from_value(serde_json::json!({"version":version})).unwrap();
            let error = service
                .update_stage("unread-record", req, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("无资格不得探测存在性、版本或供应商角色");
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }

    #[tokio::test]
    async fn create_authorizes_before_party_or_supplier_reads() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let req = serde_json::from_value(serde_json::json!({"address_type":"operating","contact_name":"name","address":"address","valid_from":"2026-01-01","is_default":false})).unwrap();
        let error = service
            .create_stage("unread-party", req, &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("无资格不得探测主体或供应商角色");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
