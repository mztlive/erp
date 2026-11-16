//! 域 D07 `party_contact` 服务编排。
//!
//! 联系人按「有效期事实追加」维护（W03），内容变更不原地修改；
//! 手机号是低熵敏感值（§4.5.5）：实体只保存带密钥 HMAC 指纹与密文，
//! 指纹密钥见 `super::sensitive`。原地更新只允许切换启停状态、结束有效期
//! 与调整默认标记；同一主体默认联系人唯一（跨行约束，事务内校验，§6.2）。

use std::sync::Arc;

use application_core::{AuditActor, normalized_text};
use erp_core::field_update::FieldUpdate;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::party;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::clear_default_marks;
use super::sensitive::SensitiveDataCodec;
use crate::dto::party::{
    CreatePartyContactRequest, PARTY_CONTACT_SORT_FIELDS, PageView, PartyContactListParams, PartyContactView,
    SortDir, UpdatePartyContactRequest, normalize_paging,
};
use crate::entity::party::{
    EffectiveRecordStatus, PartyContact, PartyContactData, PartyContactId, PartyContactUpdate, PartyId,
};
use crate::error::{Error, Result};
use crate::ports::{PartyAuditPort, SupplierRolePort};
use crate::repository::PartyExt;
use crate::repository::prelude::*;

/// 联系人列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyContactFilter = <mongodb::Database as PartyExt>::PartyContactFilter;

/// 联系人服务。
#[derive(Clone)]
pub struct PartyContactService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    audit: Arc<dyn PartyAuditPort>,
    supplier_roles: Arc<dyn SupplierRolePort>,
    authorization: Arc<AuthorizationService>,
}

impl PartyContactService {
    /// 创建联系人服务实例。
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
        actor: &AuditActor,
        party_id: &str,
        params: &PartyContactListParams,
    ) -> Result<PageView<PartyContactView>> {
        let service = self.clone();
        let actor = actor.clone();
        let party_id = party_id.to_owned();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.party_contact_list_snapshot(&actor, &party_id, &params, executor).await
                })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn party_contact_list_snapshot(
        &self,
        actor: &AuditActor,
        party_id: &str,
        params: &PartyContactListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<PartyContactView>> {
        self.authorization.require(actor, &party::CONTACT_LIST, AccessTarget::Operation, executor).await?;
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
            PARTY_CONTACT_SORT_FIELDS,
        )?;
        let filter = PartyContactFilter {
            party_id: Some(PartyId::new(party_id)),
            keyword: normalized_text(params.keyword.as_deref()),
            mobile_query_hmac: None,
            status: params.status,
            is_default: params.is_default,
            page: paging.page,
            page_size: paging.page_size,
            sort_by: Some(paging.sort_by.to_string()),
            sort_ascending: matches!(paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.party_contacts().search_party_contacts(&filter, executor).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyContactView {
                id: row.id,
                party_id: row.party_id.to_string(),
                contact_name: row.contact_name,
                title: row.title,
                telephone: row.telephone,
                mobile_masked: crate::dto::party::masked_last4(&row.mobile_last4),
                email: row.email,
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
        req: CreatePartyContactRequest,
        actor: &AuditActor,
    ) -> Result<PartyContact> {
        let mobile = req.mobile.clone();
        let mut contact = PartyContact::new(
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
            self.sensitive_data.fingerprint_key(),
            actor.id(),
        )?;
        contact.mobile_ciphertext = self.sensitive_data.encrypt(&mobile)?;
        Ok(contact)
    }

    /// 在授权快照中核验主体和供应商边界，再原子写入默认标记、事实与审计。
    async fn create_stage(
        &self,
        party_id: &str,
        req: CreatePartyContactRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PartyContactView> {
        self.authorization.require(actor, &party::CONTACT_CREATE, AccessTarget::Operation, executor).await?;
        super::ensure_new_fact_guards(&self.db, self.supplier_roles.as_ref(), party_id, executor).await?;
        let contact = self.prepare_record(party_id, req, actor)?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party_contact.create",
            "party_contact",
            contact.base.id.clone(),
        )?;
        if contact.is_default {
            clear_default_marks!(self.db, party_contacts, contact.party_id, None, executor);
        }
        self.db.party_contacts().create(&contact, executor).await?;
        self.audit.persist(&audit, executor).await?;
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
        req: UpdatePartyContactRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PartyContactView> {
        self.authorization.require(actor, &party::CONTACT_UPDATE, AccessTarget::Operation, executor).await?;
        let mut contact = self
            .db
            .party_contacts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("联系人不存在".into()))?;
        super::ensure_outside_supplier_profile(self.supplier_roles.as_ref(), &contact.party_id, executor)
            .await?;
        super::map_version_conflict(contact.ensure_version(req.version))?;
        contact.update(
            PartyContactUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party_contact.update",
            "party_contact",
            contact.base.id.clone(),
        )?;
        if contact.is_default {
            clear_default_marks!(self.db, party_contacts, contact.party_id, Some(&contact.base.id), executor);
        }
        self.db.party_contacts().update(&mut contact, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(contact.into())
    }
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, FailClosedSupplierRolePort};

    async fn service() -> PartyContactService {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("party_contact_authorization");
        PartyContactService::new(
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
        let req = serde_json::from_value(serde_json::json!({"contact_name":"name","mobile":"13800000000","valid_from":"2026-01-01","is_default":false})).unwrap();
        let error = service
            .create_stage("unread-party", req, &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("无资格不得探测主体或供应商角色");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
