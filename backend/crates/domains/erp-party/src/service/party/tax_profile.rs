//! 域 D07 `party_tax_profile` 服务编排。
//!
//! 税务资料按「有效期事实追加」维护（§6.2），税号变更不原地修改；
//! 原地更新只允许切换启停状态、结束有效期与调整默认标记；同一主体默认
//! 税务资料唯一（跨行约束，事务内校验，§6.2）。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::field_update::FieldUpdate;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::party;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use crate::dto::party::{
    CreatePartyTaxProfileRequest, PARTY_TAX_PROFILE_SORT_FIELDS, PageView, PartyTaxProfileListParams,
    PartyTaxProfileView, SortDir, UpdatePartyTaxProfileRequest, normalize_paging,
};
use crate::entity::party::{
    EffectiveRecordStatus, PartyId, PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileId,
    PartyTaxProfileUpdate,
};
use crate::error::{Error, Result};
use crate::ports::{PartyAuditPort, SupplierRolePort};
use crate::repository::prelude::*;
use crate::repository::{PartyExt, PartyTaxProfileFilter};

/// 税务资料服务。
#[derive(Clone)]
pub struct PartyTaxProfileService {
    db: Database,
    audit: Arc<dyn PartyAuditPort>,
    supplier_roles: Arc<dyn SupplierRolePort>,
    authorization: Arc<AuthorizationService>,
}

impl PartyTaxProfileService {
    /// 创建税务资料服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
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
        audit: Arc<dyn PartyAuditPort>,
        supplier_roles: Arc<dyn SupplierRolePort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, audit, supplier_roles, authorization }
    }

    /// 分页查询税务资料列表。
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
    pub async fn party_tax_profile_list(
        &self,
        actor: &AuditActor,
        party_id: &str,
        params: &PartyTaxProfileListParams,
    ) -> Result<PageView<PartyTaxProfileView>> {
        let service = self.clone();
        let actor = actor.clone();
        let party_id = party_id.to_owned();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.party_tax_profile_list_snapshot(&actor, &party_id, &params, executor).await
                })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn party_tax_profile_list_snapshot(
        &self,
        actor: &AuditActor,
        party_id: &str,
        params: &PartyTaxProfileListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<PartyTaxProfileView>> {
        self.authorization
            .require(actor, &party::TAX_PROFILE_LIST, AccessTarget::Operation, executor)
            .await?;
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
            PARTY_TAX_PROFILE_SORT_FIELDS,
        )?;
        let filter = PartyTaxProfileFilter {
            party_id: Some(PartyId::new(party_id)),
            status: params.status,
            is_default: params.is_default,
            page: paging.page,
            page_size: paging.page_size,
            sort_by: Some(paging.sort_by.to_string()),
            sort_ascending: matches!(paging.sort_dir, SortDir::Asc),
        };

        let page = self.db.party_tax_profiles().search_party_tax_profiles(&filter, executor).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyTaxProfileView {
                id: row.id,
                party_id: row.party_id.to_string(),
                tax_no: row.tax_no,
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

    /// 创建税务资料（跨行事务：默认资料唯一 + 新建 + 审计原子写入）。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建税务资料的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_party_tax_profile(
        &self,
        party_id: &str,
        req: CreatePartyTaxProfileRequest,
        actor: &AuditActor,
    ) -> Result<PartyTaxProfileView> {
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
        req: CreatePartyTaxProfileRequest,
        actor: &AuditActor,
    ) -> Result<PartyTaxProfile> {
        let profile = PartyTaxProfile::new(
            PartyTaxProfileId::new(next_id()),
            PartyTaxProfileData {
                party_id: PartyId::new(party_id),
                tax_no: req.tax_no,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                is_default: req.is_default,
                status: req.status.unwrap_or(EffectiveRecordStatus::Active),
            },
            actor.id(),
        )?;
        Ok(profile)
    }

    /// 在授权快照中核验主体和供应商边界，再原子写入默认标记、事实与审计。
    async fn create_stage(
        &self,
        party_id: &str,
        req: CreatePartyTaxProfileRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PartyTaxProfileView> {
        self.authorization
            .require(actor, &party::TAX_PROFILE_CREATE, AccessTarget::Operation, executor)
            .await?;
        super::ensure_new_fact_guards(&self.db, self.supplier_roles.as_ref(), party_id, executor).await?;

        let profile = self.prepare_record(party_id, req, actor)?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party_tax_profile.create",
            "party_tax_profile",
            profile.base.id.clone(),
        )?;

        if profile.is_default {
            self.db.party_tax_profiles().clear_other_default_marks(&profile.party_id, None, executor).await?;
        }
        self.db.party_tax_profiles().create(&profile, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(profile.into())
    }

    /// 更新税务资料（仅生命周期字段；默认标记跨行事务）。
    ///
    /// # 参数
    /// * `id` - 税务资料 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后税务资料的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 税务资料不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_party_tax_profile(
        &self,
        id: &str,
        req: UpdatePartyTaxProfileRequest,
        actor: &AuditActor,
    ) -> Result<PartyTaxProfileView> {
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
        req: UpdatePartyTaxProfileRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PartyTaxProfileView> {
        self.authorization
            .require(actor, &party::TAX_PROFILE_UPDATE, AccessTarget::Operation, executor)
            .await?;
        let mut profile = self
            .db
            .party_tax_profiles()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("税务资料不存在".into()))?;
        super::ensure_outside_supplier_profile(self.supplier_roles.as_ref(), &profile.party_id, executor)
            .await?;
        super::map_version_conflict(profile.ensure_version(req.version))?;

        profile.update(
            PartyTaxProfileUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party_tax_profile.update",
            "party_tax_profile",
            profile.base.id.clone(),
        )?;

        if profile.is_default {
            self.db
                .party_tax_profiles()
                .clear_other_default_marks(&profile.party_id, Some(profile.base.id.as_str()), executor)
                .await?;
        }
        self.db.party_tax_profiles().update(&mut profile, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(profile.into())
    }
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, FailClosedSupplierRolePort};

    async fn service() -> PartyTaxProfileService {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("party_tax_profile_authorization");
        PartyTaxProfileService::new(
            db,
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
        let req = serde_json::from_value(
            serde_json::json!({"tax_no":"tax","valid_from":"2026-01-01","is_default":false}),
        )
        .unwrap();
        let error = service
            .create_stage("unread-party", req, &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("无资格不得探测主体或供应商角色");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
