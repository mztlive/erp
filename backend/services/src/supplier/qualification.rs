//! 域 D09 `supplier_qualification` 服务编排。
//!
//! 资质独立保存，一家供应商维护多份资质（§4.5）；资质修订内联结构化快照
//! （§2.2/§4.4），更新时追加资质修订并推进表头 `current_revision_id`；
//! 资质 ↔ 适用能力通过 `supplier_qualification_capability` 纯关联行表达，
//! 创建时在同一事务写入（§6.2）。
//!
//! 跨域：资质附件（D05 `file_asset`）创建时校验存在。
//! **已知缺口**：适用能力关联行暂无「整体替换」仓储入口（需地基修订增加
//! `replace_qualification_capabilities`），本阶段更新不修改关联行。

use database::{AccessControlExt, FileAssetExt, NoTransaction, SupplierExt, Transactional};
use entities::common::revision::RevisionBase;
use entities::field_update::FieldUpdate;
use entities::ids::{FileAssetId, SupplierAccountId, SupplierCapabilityId};
use entities::supplier::{
    QualificationStatus, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationCapabilityData, SupplierQualificationCapabilityId, SupplierQualificationData,
    SupplierQualificationId, SupplierQualificationRevision, SupplierQualificationRevisionData,
    SupplierQualificationRevisionId, SupplierQualificationUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    attachment_update, normalize_sort, CreateSupplierQualificationRequest, PageView, SortDir,
    SupplierQualificationListParams, SupplierQualificationView, UpdateSupplierQualificationRequest,
    QUALIFICATION_SORT_FIELDS,
};
use super::{page_or_default, page_size_or_default};

/// 资质列表筛选条件类型（经 `SupplierExt` 关联类型跨 crate 可达）。
type SupplierQualificationFilter = <mongodb::Database as SupplierExt>::SupplierQualificationFilter;

/// 供应商资质服务。
pub struct SupplierQualificationService {
    db: Database,
}

impl SupplierQualificationService {
    /// 创建供应商资质服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商资质列表。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn supplier_qualification_list(
        &self,
        supplier_id: &str,
        params: &SupplierQualificationListParams,
    ) -> Result<PageView<SupplierQualificationView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            normalize_sort(&params.sort_by, &params.sort_dir, QUALIFICATION_SORT_FIELDS)?;
        let filter = SupplierQualificationFilter {
            supplier_id: Some(SupplierAccountId::new(supplier_id)),
            qualification_type: params.qualification_type,
            status: params.status,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_qualifications()
            .search_supplier_qualifications(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierQualificationView {
                id: row.id,
                supplier_id: row.supplier_id.to_string(),
                qualification_type: row.qualification_type,
                certificate_no: row.certificate_no,
                issuer: row.issuer,
                valid_from: row.valid_from,
                valid_to: row.valid_to,
                attachment_id: row.attachment_id,
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

    /// 创建供应商资质（跨集合事务：资质 + 首版修订 + 适用能力关联 + 审计）。
    ///
    /// 适用能力必须属于该供应商且存在（§6.2：`supplier_qualification_capability`
    /// 明确适用能力）；证书编号组合唯一由唯一索引拦截（409）。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建资质的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商、附件或适用能力不存在
    /// * `ConflictError` - 证书编号组合重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_supplier_qualification(
        &self,
        supplier_id: &str,
        req: CreateSupplierQualificationRequest,
        actor: &AuditActor,
    ) -> Result<SupplierQualificationView> {
        req.validate()?;
        let supplier_id_typed = SupplierAccountId::new(supplier_id);
        self.ensure_supplier_exists(&supplier_id_typed).await?;
        if let Some(attachment_id) = &req.attachment_id {
            self.ensure_attachment_exists(attachment_id).await?;
        }
        for capability_id in &req.capability_ids {
            self.ensure_capability_belongs_to(&supplier_id_typed, capability_id)
                .await?;
        }

        let qualification_id = SupplierQualificationId::new(next_id());
        let revision_id = SupplierQualificationRevisionId::new(next_id());
        let mut qualification = SupplierQualification::new(
            qualification_id.clone(),
            SupplierQualificationData {
                supplier_id: supplier_id_typed.clone(),
                qualification_type: req.qualification_type,
                certificate_no: req.certificate_no,
                issuer: req.issuer,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                attachment_id: req.attachment_id,
                status: req.status.unwrap_or(QualificationStatus::Active),
            },
            actor.id(),
        )?;
        qualification.stable.current_revision_id = Some(revision_id.to_string());
        let revision = SupplierQualificationRevision::new(
            revision_id,
            SupplierQualificationRevisionData {
                supplier_id: supplier_id_typed.clone(),
                qualification_type: qualification.qualification_type,
                certificate_no: qualification.certificate_no.clone(),
                issuer: qualification.issuer.clone(),
                valid_from: qualification.valid_from,
                valid_to: qualification.valid_to,
                attachment_id: qualification.attachment_id.clone(),
                status: qualification.stable.status,
                revision_no: 1,
            },
        )?;
        let links: Vec<SupplierQualificationCapability> = req
            .capability_ids
            .into_iter()
            .map(|capability_id| {
                SupplierQualificationCapability::new(
                    SupplierQualificationCapabilityId::new(next_id()),
                    SupplierQualificationCapabilityData {
                        qualification_id: qualification_id.clone(),
                        capability_id,
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()?;
        let audit = actor.clone().resource_log(
            "supplier_qualification.create",
            "supplier_qualification",
            qualification.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let qualification_for_tx = qualification.clone();
        let revision_for_tx = revision.clone();
        let links_for_tx = links.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_qualification_revisions()
                        .create(&revision_for_tx, session)
                        .await?;
                    db.supplier_qualifications()
                        .create(&qualification_for_tx, session)
                        .await?;
                    for link in &links_for_tx {
                        db.supplier_qualification_capabilities()
                            .create(link, session)
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(qualification.into())
    }

    /// 更新供应商资质（乐观锁；跨集合事务：追加资质修订 + CAS 更新资质 + 审计）。
    ///
    /// 内容变更形成新修订快照；`attachment_id` 为空字符串表示清除附件。
    ///
    /// # 参数
    /// * `id` - 资质 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后资质的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 资质或附件不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    pub async fn update_supplier_qualification(
        &self,
        id: &str,
        req: UpdateSupplierQualificationRequest,
        actor: &AuditActor,
    ) -> Result<SupplierQualificationView> {
        req.validate()?;
        let mut qualification = self
            .db
            .supplier_qualifications()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("资质不存在".to_string()))?;
        if qualification.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let attachment_update = match attachment_update(req.attachment_id) {
            Some(Some(attachment_id)) => {
                self.ensure_attachment_exists(&attachment_id).await?;
                FieldUpdate::Set(attachment_id)
            }
            Some(None) => FieldUpdate::Clear,
            None => FieldUpdate::Unchanged,
        };
        qualification.update(
            SupplierQualificationUpdate {
                issuer: option_update(req.issuer),
                attachment_id: attachment_update,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "supplier_qualification.update",
            "supplier_qualification",
            qualification.base.id.clone(),
        )?;
        let updated_by = actor.id().to_string();

        let db = self.db.clone();
        let client = db.client().clone();
        let mut qualification_for_tx = qualification.clone();
        let revision_for_tx = SupplierQualificationRevision::new(
            SupplierQualificationRevisionId::new(next_id()),
            SupplierQualificationRevisionData {
                supplier_id: qualification.supplier_id.clone(),
                qualification_type: qualification.qualification_type,
                certificate_no: qualification.certificate_no.clone(),
                issuer: qualification.issuer.clone(),
                valid_from: qualification.valid_from,
                valid_to: qualification.valid_to,
                attachment_id: qualification.attachment_id,
                status: qualification.stable.status,
                revision_no: 0,
            },
        )?;
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let history = db
                        .supplier_qualification_revisions()
                        .find_many(
                            ::mongodb::bson::doc! {
                                "supplier_id": qualification_for_tx.supplier_id.to_string(),
                                "qualification_type": qualification_for_tx.qualification_type.as_str(),
                                "certificate_no": &qualification_for_tx.certificate_no,
                            },
                            session,
                        )
                        .await?;
                    let next_no = history
                        .iter()
                        .map(|revision| revision.revision.revision_no)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    let revision = SupplierQualificationRevision {
                        revision: RevisionBase::new(next_no),
                        ..revision_for_tx
                    };
                    qualification_for_tx.stable.current_revision_id = Some(revision.base.id.clone());
                    qualification_for_tx.stable.touch(&updated_by);
                    db.supplier_qualification_revisions()
                        .create(&revision, session)
                        .await?;
                    db.supplier_qualifications()
                        .update(&mut qualification_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierQualification, crate::errors::Error>(qualification_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验供应商角色存在。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    ///
    /// # 返回
    /// 供应商存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    async fn ensure_supplier_exists(&self, supplier_id: &SupplierAccountId) -> Result<()> {
        self.db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        Ok(())
    }

    /// 校验资质附件存在（D05 跨域读）。
    ///
    /// # 参数
    /// * `attachment_id` - 文件资产 ID
    ///
    /// # 返回
    /// 附件存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 附件不存在
    async fn ensure_attachment_exists(&self, attachment_id: &FileAssetId) -> Result<()> {
        self.db
            .file_assets()
            .find_by_id(attachment_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("资质附件不存在，请先上传文件".to_string()))?;
        Ok(())
    }

    /// 校验适用能力属于该供应商（§6.2 资质 ↔ 能力关联）。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `capability_id` - 能力 ID
    ///
    /// # 返回
    /// 能力存在且属于该供应商时返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 能力不存在或不属于该供应商
    async fn ensure_capability_belongs_to(
        &self,
        supplier_id: &SupplierAccountId,
        capability_id: &SupplierCapabilityId,
    ) -> Result<()> {
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_id(capability_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("适用能力不存在".to_string()))?;
        if &capability.supplier_id != supplier_id {
            return Err(Error::NotFound("适用能力不属于该供应商".to_string()));
        }
        Ok(())
    }
}

/// 将可选文本入参映射为 `FieldUpdate`：`None` 表示不修改，空字符串表示清除。
///
/// # 参数
/// * `value` - 请求携带的文本
///
/// # 返回
/// 返回实体更新意图。
fn option_update(value: Option<String>) -> FieldUpdate<String> {
    match value {
        Some(raw) if raw.trim().is_empty() => FieldUpdate::Clear,
        Some(raw) => FieldUpdate::Set(raw),
        None => FieldUpdate::Unchanged,
    }
}
