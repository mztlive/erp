//! 登记导入文件资产并创建后台任务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::entity::catalog::product_import::{collapse_import_text, truncate_import_text};
use erp_catalog::{PRODUCT_IMPORT_NAME_COLUMN, PRODUCT_IMPORT_SHEET_NAME, ProductImportJobView};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::internal;
use erp_support::repository::bulk_job::BackgroundJobInput;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobItemId, BackgroundJobRegistration, BulkJobExt, FileAsset,
    FileAssetExt, FileAssetId, JobType, PRODUCT_IMPORT_DOMAIN_JOB_TYPE, RegisterFileAssetRequest,
    product_import_job_no,
};
use id_generator::next_id;
use persistence_core::{Error as PersistenceError, Executor, Transactional};

use super::ProductImportProcess;
use super::parse::{ParsedProductSheet, parse_product_quote_xlsx};
use super::row_manifest::{build_row_manifest, delete_manifest_objects, prepare_row_manifest};
use super::views::job_view;
use crate::{Error, Result};

impl ProductImportProcess {
    /// 表单上传商品报价表后，先证明当前账号有商品创建资格，再解析原模板并按原请求身份登记异步任务。
    /// 文件须已写入对象存储；仍需原字节来解析和准备行媒体，并用登记请求创建资产事实。
    ///
    /// 返回新任务或幂等回放的既有任务。资格不足、文件不是原模板、没有数据行或登记失败时返回错误。
    pub async fn submit(
        &self,
        file_name: String,
        bytes: Vec<u8>,
        registration: RegisterFileAssetRequest,
        request_id: String,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        self.prepare_attachment_upload(actor).await?;

        let shared_bytes = Arc::new(bytes);
        let for_parse = shared_bytes.clone();
        let parsed = tokio::task::spawn_blocking(move || parse_product_quote_xlsx(&for_parse))
            .await
            .map_err(|_| Error::Internal("解析导入文件失败".into()))??;
        parsed.ensure_supply_template()?;

        let file_asset = FileAsset::new(FileAssetId::new(next_id()), registration.into_data(actor.id())?)?;
        self.create_job_from_parsed(file_asset, parsed, file_name, request_id, actor, shared_bytes).await
    }

    // 表单上传和浏览器直传共用此入口：解析完成后，沿用原请求身份从本次源字节预提各行图片并登记任务。
    // 源文件须已存入对象存储；小清单随任务原子提交，较大清单存对象存储。返回新任务或原幂等结果。
    //
    // 装配、清单或登记失败按原错误返回；清单或登记失败时尝试删除本次已上传对象，但 `OutcomeUnknown` 不补偿。
    pub(super) async fn create_job_from_parsed(
        &self,
        file_asset: FileAsset,
        parsed: ParsedProductSheet,
        file_name: String,
        request_id: String,
        actor: &AuditActor,
        xlsx: Arc<Vec<u8>>,
    ) -> Result<ProductImportJobView> {
        let (job, items) = import_job(&parsed, &file_asset, &request_id, actor)?;

        let built = build_row_manifest(&self.storage, &self.secret, &request_id, &parsed, xlsx).await?;
        let (input, manifest_key) = match prepare_row_manifest(&self.storage, &built.manifest).await {
            Ok(prepared) => prepared,
            Err(error) => {
                delete_manifest_objects(&self.storage, &built.uploaded_object_keys).await;
                return Err(error);
            },
        };

        let mut manifest_keys = built.uploaded_object_keys;
        manifest_keys.extend(manifest_key);
        match self.persist_job(file_asset, job, items, file_name, input, actor).await {
            Ok(view) => Ok(view),
            Err(error) => {
                if !matches!(error, Error::OutcomeUnknown(_)) {
                    delete_manifest_objects(&self.storage, &manifest_keys).await;
                }
                Err(error)
            },
        }
    }

    // 媒体及源文件已准备后，在同一登记事务重验提交人的商品创建资格，再保存文件资产、导入任务、逐行明细及可选私有清单。
    //
    // - `file_asset`：已落对象存储的文件资产。
    // - `job`：待登记的导入任务。
    // - `items`：逐行明细。
    // - `input`：已通过容量校验的可选私有清单。
    // - `actor`：提交人，任务登记前重验当前商品创建资格。
    //
    // 返回新任务或既有幂等回放任务。
    //
    // 当前账号失效或缺少商品创建资格时拒绝；事务、唯一键或异载荷冲突按原分类传播。
    // 未知提交不执行对象补偿。
    pub(super) async fn persist_job(
        &self,
        file_asset: FileAsset,
        job: BackgroundJob,
        items: Vec<BackgroundJobItem>,
        file_name: String,
        input: Option<BackgroundJobInput>,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let file_for_tx = file_asset.clone();
        let job_for_tx = job.clone();
        let authorization = self.job_access.service.clone();
        let operator = actor.clone();
        let registration = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorize(&authorization, &operator, executor).await?;

                    db.file_assets().create(&file_for_tx, executor).await.map_err(registration_error)?;
                    let registration = db
                        .bulk_job()
                        .create_job_with_items(&job_for_tx, items, executor)
                        .await
                        .map_err(registration_error)?;
                    if let Some(input) = input {
                        let job_id = BackgroundJobId::new(job_for_tx.base.id.clone());
                        db.bulk_job()
                            .create_job_input(&job_id, &input, executor)
                            .await
                            .map_err(registration_error)?;
                    }
                    Ok::<_, Error>(registration)
                })
            })
            .await;

        match registration {
            Ok(BackgroundJobRegistration::Created) => Ok(job_view(&job, Some(file_name))),
            Ok(BackgroundJobRegistration::ReplaySame(existing)) => Ok(job_view(&existing, Some(file_name))),
            Ok(BackgroundJobRegistration::ConflictDifferentPayload(_)) => {
                Err(Error::ConflictError("本次提交已用于另一份导入，请刷新任务列表并核对原任务".into()))
            },
            Err(Error::RepositoryError(PersistenceError::DuplicateKey(_))) => {
                self.replay_existing_job(&job.request_id, actor).await
            },
            Err(error) => Err(error),
        }
    }

    // 商品导入登记遇到唯一键竞争时，以原请求身份查证既有任务；查证前仍要重验提交人的商品创建资格。
    //
    // `request_id`：幂等请求身份；`actor`：提交人，回放前重验当前商品创建资格。
    //
    // 返回已存在任务的视图；文件名不随回放恢复。
    //
    // 当前账号失效、缺少商品创建资格或查询失败时拒绝。找不到任务时返回 `ConflictError`。
    pub(super) async fn replay_existing_job(
        &self,
        request_id: &str,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        let db = self.db.clone();
        let authorization = self.job_access.service.clone();
        let operator = actor.clone();
        let request_id = request_id.to_string();
        let existing = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorize(&authorization, &operator, executor).await?;
                    db.background_jobs().find_by_request_id(&request_id, executor).await?.ok_or_else(|| {
                        Error::ConflictError(
                            "导入任务正在由另一请求登记，请刷新任务列表并核对是否已提交".into(),
                        )
                    })
                })
            })
            .await?;
        Ok(job_view(&existing, None))
    }

    /// 在短快照内证明当前入口资格；解析与对象存储操作在该快照成功后执行。
    ///
    /// actor 为已认证提交人。
    ///
    /// 当前账号具有商品创建功能资格时返回成功。
    ///
    /// 身份、资格取证或快照事务失败时拒绝后续处理。
    pub async fn prepare_attachment_upload(&self, actor: &AuditActor) -> Result<()> {
        let authorization = self.job_access.service.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { authorize(&authorization, &actor, executor).await })
            })
            .await
    }
}

// 导入入口只检查功能资格；商品候选范围由后台逐行创建重新验证。
async fn authorize(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization.require(actor, &internal::PRODUCT_IMPORT_SUBMIT, AccessTarget::Operation, executor).await?;
    Ok(())
}

// 保留唯一键竞争的类型，交给登记入口回读；其余持久化错误沿用统一分类。
fn registration_error(error: PersistenceError) -> Error {
    match error {
        error @ PersistenceError::DuplicateKey(_) => Error::RepositoryError(error),
        error => error.into(),
    }
}

// 用原模板行顺序构造任务明细，保留行号、名称与独立明细身份。
fn import_item_drafts(parsed: &ParsedProductSheet) -> Vec<BackgroundJobItemDraft> {
    parsed
        .rows
        .iter()
        .map(|row| {
            let name = row.cells.get(PRODUCT_IMPORT_NAME_COLUMN - 1).and_then(|value| {
                let text = collapse_import_text(value);
                if text.is_empty() { None } else { Some(truncate_import_text(&text, 128)) }
            });
            BackgroundJobItemDraft {
                id: BackgroundJobItemId::new(next_id()),
                object_type: name.as_ref().map(|_| "product_import_row".to_string()),
                object_id: name,
                expected_version: None,
                expected_hash: None,
                worksheet_name: Some(PRODUCT_IMPORT_SHEET_NAME.to_string()),
                source_row_no: Some(row.row_number),
                source_column_name: None,
            }
        })
        .collect()
}

// 由原领域聚合构造任务及明细，业务校验仍由后台任务实体执行。
fn import_job(
    parsed: &ParsedProductSheet,
    file_asset: &FileAsset,
    request_id: &str,
    actor: &AuditActor,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let job_id = BackgroundJobId::new(next_id());
    let drafts = import_item_drafts(parsed);
    let total_rows = u64::try_from(drafts.len()).expect("导入行数不超过 1000");
    let aggregate = BackgroundJobAggregate::new(
        job_id,
        BackgroundJobAggregateData {
            job_no: product_import_job_no(request_id),
            job_type: JobType::Import,
            domain_job_type: Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE.to_string()),
            domain_job_id: Some(file_asset.base.id.clone()),
            selection_snapshot_id: None,
            requested_by: actor.id().to_string(),
            request_id: request_id.to_string(),
            input_file_asset_id: Some(FileAssetId::new(file_asset.base.id.clone())),
            result_file_asset_id: None,
            declared_total_count: total_rows,
        },
        drafts,
    )?;
    Ok(aggregate.into_parts())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::Error as IdentityError;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::entity::authorization::{FactValue, ObjectFacts};
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_permissions::{Permission, catalog};
    use mongodb::error::Error as MongoError;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("importer".into(), "operator".into(), AccountKind::Admin)
    }

    fn authorization(permissions: &[Permission]) -> Arc<AuthorizationService> {
        fixed_authorization(
            permissions,
            ResolvedScope { role_clauses: Vec::new(), user_limit: None },
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn import_entry_requires_current_product_creation_permission() {
        for permissions in
            [vec![], vec![catalog::PRODUCT_PRODUCT_LIST], vec![internal::PRODUCT_IMPORT_SUBMIT]]
        {
            let denied = authorization(&permissions);
            assert!(matches!(
                authorize(&denied, &actor(), &mut SnapshotExecutor::default()).await,
                Err(Error::Forbidden(_))
            ));
        }
        let allowed = authorization(&[catalog::PRODUCT_PRODUCT_CREATE]);
        authorize(&allowed, &actor(), &mut SnapshotExecutor::default()).await.unwrap();
        let unavailable = unavailable_authorization();
        assert!(matches!(
            authorize(&unavailable, &actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(message)) if message == "测试授权设施不可用"
        ));
    }

    #[tokio::test]
    async fn import_entry_rejects_non_snapshot_executors() {
        let allowed = authorization(&[catalog::PRODUCT_PRODUCT_CREATE]);
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            assert!(matches!(
                authorize(&allowed, &actor(), executor).await,
                Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
            ));
        }
    }

    #[tokio::test]
    async fn import_qualification_does_not_replace_product_candidate_scope() {
        let allowed = authorization(&[catalog::PRODUCT_PRODUCT_CREATE]);
        let mut executor = SnapshotExecutor::default();
        authorize(&allowed, &actor(), &mut executor).await.unwrap();
        assert!(matches!(
            allowed
                .require(&actor(), &catalog::PRODUCT_PRODUCT_CREATE, AccessTarget::Operation, &mut executor)
                .await,
            Err(IdentityError::ValidationError(_))
        ));
        let candidate = ObjectFacts {
            resource: "product".into(),
            owner_id: FactValue::Known(Some("importer".into())),
            org_unit_id: FactValue::Known(None),
            ..Default::default()
        };
        assert!(matches!(
            allowed
                .require(
                    &actor(),
                    &catalog::PRODUCT_PRODUCT_CREATE,
                    AccessTarget::Candidate(candidate),
                    &mut executor
                )
                .await,
            Err(IdentityError::Forbidden(_))
        ));
    }

    #[test]
    fn registration_keeps_unique_conflicts_replayable_and_other_error_classes() {
        let duplicate = PersistenceError::DuplicateKey(MongoError::custom("duplicate"));
        assert!(matches!(
            registration_error(duplicate),
            Error::RepositoryError(PersistenceError::DuplicateKey(_))
        ));
        assert!(matches!(
            registration_error(PersistenceError::OptimisticLockingError),
            Error::ConflictError(_)
        ));
    }
}
