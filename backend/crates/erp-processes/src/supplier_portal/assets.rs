//! 图片所有权和保留事实校验；数据库步骤不执行对象存储I/O。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_catalog::portal::NewProductInput;
use erp_core::common::time::Instant;
use erp_core::ids::FileAssetId;
use erp_identity::{PortalActor, PortalIdentityService};
use erp_read_models::supplier_portal::{PortalSkuImageSource, portal_sku_image_source};
use erp_supply::portal::{PortalOfferingService, PortalSupplyExt};
use erp_support::FileAssetExt;
use erp_support::entity::file_asset::{FileAsset, FileAssetData, RetentionClass};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::SupplierPortalProcess;
use super::authorization::request_readable;
use super::command::scoped_command;
use crate::{Error, Result};

/// 供应商允许看到的上传结果，不返回对象存储键或内容指纹。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalAssetView {
    pub id: String,
    pub version: u64,
    pub file_name: String,
    pub content_type: String,
    pub asset_kind: PortalAssetKind,
    pub byte_size: u64,
    pub request_version: u64,
    #[serde(skip)]
    pub assets_committed: bool,
}

/// 上传内容种类；资料附件不得作为公开商品图片。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PortalAssetKind {
    /// 已完整解码的商品图片。
    Image,
    /// 已严格检查的静态 PDF 资料。
    #[serde(rename = "DOCUMENT")]
    Attachment,
}

/// 当前图片来源的完整版本快照，供下载前后复核。
pub struct PortalCatalogAssetAccess {
    pub asset: FileAsset,
    pub source: PortalSkuImageSource,
    pub offering_version: Option<u64>,
}

/// 由 HTTP 完成有界图像解码或严格 PDF 检查后传入，禁止外部反序列化。
pub struct PreparedPortalAsset {
    data: FileAssetData,
    kind: PortalAssetKind,
}
impl PreparedPortalAsset {
    /// 接收可信组合层已经完整解码的图像及安全文件记录。
    /// # 参数
    /// 受控元数据和真实解码得到的宽高，禁止取自请求自报尺寸。
    /// # 返回
    /// 返回可登记的准备对象。
    /// # 错误
    /// MIME、大小、边长或零尺寸不满足门户图片合同则拒绝。
    pub fn from_decoded_image(data: FileAssetData, width: u32, height: u32) -> Result<Self> {
        if !matches!(data.content_type.as_str(), "image/png" | "image/jpeg" | "image/webp")
            || data.byte_size == 0
            || data.byte_size > 5 * 1024 * 1024
            || width == 0
            || height == 0
            || width > 8192
            || height > 8192
        {
            return Err(Error::ValidationError("门户图片格式、大小或真实尺寸无效".into()));
        }
        Ok(Self { data, kind: PortalAssetKind::Image })
    }

    /// 接收可信组合层严格解析并检查完成的静态 PDF 资料。
    /// # 参数
    /// 受控文件元数据与真实解析验证的页面数，不接受客户端声明。
    /// # 返回
    /// 仅可登记为资料附件的准备对象。
    /// # 错误
    /// MIME、大小或页面数不满足门户资料合同则拒绝。
    pub fn from_checked_pdf(data: FileAssetData, page_count: u32) -> Result<Self> {
        if data.content_type != "application/pdf"
            || data.byte_size == 0
            || data.byte_size > 5 * 1024 * 1024
            || page_count == 0
            || page_count > 500
        {
            return Err(Error::ValidationError("门户资料须为 5 MiB 内的有效静态 PDF".into()));
        }
        Ok(Self { data, kind: PortalAssetKind::Attachment })
    }
}

impl PortalAssetKind {
    fn from_content_type(content_type: &str) -> Result<Self> {
        match content_type {
            "image/jpeg" | "image/png" | "image/webp" => Ok(Self::Image),
            "application/pdf" => Ok(Self::Attachment),
            _ => Err(Error::ValidationError("门户素材仅支持 JPEG、PNG、WebP 图片及静态 PDF 资料".into())),
        }
    }

    fn ensure_slot(self, is_image_slot: bool) -> Result<()> {
        if self != Self::Image && is_image_slot {
            return Err(Error::ValidationError("PDF 资料不得用作商品或 SKU 图片".into()));
        }
        Ok(())
    }
}

impl SupplierPortalProcess {
    /// 在供应商申请审核对象范围内读取原稿素材，不依赖普通文件列表权限。
    /// # 参数
    /// 精确申请、素材、真实内部身份与读取执行器。
    /// # 返回
    /// 返回当前或历史提交明确关联且仍可用的文件。
    /// # 错误
    /// 身份、申请读取范围、素材来源或治理状态失效时拒绝。
    pub async fn asset_review_access(
        &self,
        request_id: &str,
        file_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FileAsset> {
        self.internal_actor(actor, executor).await?;
        self.internal_permission(actor, "supplier_portal.application_detail", executor).await?;
        if !request_readable(&self.db, &self.rbac, actor, request_id, executor).await? {
            return Err(Error::NotFound("申请不存在或无权查看".into()));
        }
        let draft = self.catalog_portal().load(request_id, executor).await?;
        if !references(&draft.draft, file_id)
            && !draft.submissions.iter().any(|s| references(&s.input, file_id))
        {
            return Err(Error::NotFound("附件不存在或无权访问".into()));
        }
        self.db
            .file_assets()
            .find_by_id(file_id, executor)
            .await?
            .filter(|a| a.is_usable_at(Instant::now()))
            .ok_or_else(|| Error::NotFound("附件不存在或无权访问".into()))
    }

    /// 按自有供给或仍开放SKU读取当前商品图片。
    /// # 参数
    /// 文件、两种来源之一、门户身份与同一读取执行器。
    /// # 返回
    /// 返回来源当前修订明确引用的图片文件。
    /// # 错误
    /// 来源缺失、交叉来源、开放撤销或图片引用失效时拒绝。
    pub async fn asset_catalog_access(
        &self,
        file_id: &str,
        offering_id: Option<&str>,
        sku_id: Option<&str>,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<PortalCatalogAssetAccess> {
        let current = self.session_validate(actor, executor).await?;
        let service = PortalOfferingService::new(self.db.clone());
        let (target, offering_version) = match (offering_id, sku_id) {
            (Some(id), None) => {
                let offering = service
                    .require_owned_offering(&current.supplier_id, &current.audit_actor(), id, executor)
                    .await?;
                (offering.sku_id.to_string(), Some(offering.base.version))
            },
            (None, Some(id)) => {
                service.ensure_quote_access(&current.supplier_id, id, executor).await?;
                (id.to_string(), None)
            },
            _ => return Err(Error::NotFound("图片来源不存在或无权访问".into())),
        };
        let source = portal_sku_image_source(&self.db, &target, executor)
            .await?
            .ok_or_else(|| Error::NotFound("图片来源不存在".into()))?;
        if source.file_id != file_id {
            return Err(Error::NotFound("图片不存在或无权访问".into()));
        }
        let asset = self
            .db
            .file_assets()
            .find_by_id(file_id, executor)
            .await?
            .filter(|a| a.content_type.starts_with("image/") && a.is_usable_at(Instant::now()))
            .ok_or_else(|| Error::NotFound("图片不存在或无权访问".into()))?;
        Ok(PortalCatalogAssetAccess { asset, source, offering_version })
    }

    /// 将事务外已准备的受控文件关联当前可编辑新品草稿。
    /// # 参数
    /// 申请、已上传的安全元数据、原操作号及当前门户身份。
    /// # 返回
    /// 返回文件及草稿的新版本，不包含对象存储秘密。
    /// # 错误
    /// 越界、非可编辑申请、文件元数据非法时拒绝。
    pub async fn asset_register(
        &self,
        request_id: &str,
        expected_request_version: u64,
        prepared: PreparedPortalAsset,
        idempotency_key: &str,
        actor: &PortalActor,
    ) -> Result<PortalAssetView> {
        let PreparedPortalAsset { mut data, kind } = prepared;
        data.created_by = actor.account_id.clone();
        data.retention_class = RetentionClass::LongTerm;
        data.expires_at = None;
        let payload = asset_payload(request_id, expected_request_version, &data);
        let request_id = request_id.to_string();
        let (mut view, fresh) = self
            .portal_command_outcome(
                actor,
                "supplier_portal.asset_register",
                idempotency_key,
                &payload,
                move |this, actor, executor| {
                    Box::pin(async move {
                        let service = this.catalog_portal();
                        let draft = service.detail(&request_id, &actor.supplier_id, executor).await?;
                        let mut asset = FileAsset::new(FileAssetId::new(next_id()), data)?;
                        asset.mark_content_checked()?;
                        let mut input = draft.draft;
                        if asset.content_type.starts_with("image/") {
                            input.image_asset_ids.push(asset.base.id.clone());
                        } else {
                            input.file_asset_ids.push(asset.base.id.clone());
                        }
                        this.db.file_assets().create(&asset, executor).await?;
                        let draft = service
                            .update(
                                &request_id,
                                &actor.supplier_id,
                                expected_request_version,
                                input,
                                executor,
                            )
                            .await?;
                        Ok(PortalAssetView {
                            id: asset.base.id,
                            version: asset.base.version,
                            file_name: asset.file_name,
                            content_type: asset.content_type,
                            asset_kind: kind,
                            byte_size: asset.byte_size,
                            request_version: draft.base.version,
                            assets_committed: true,
                        })
                    })
                },
            )
            .await?;
        view.assets_committed = fresh;
        Ok(view)
    }

    /// 允许原上传命令重试时恢复原结果，随机对象键不参加指纹。
    /// # 参数
    /// 申请、原版本、安全内容元数据、原操作号及当前门户身份。
    /// # 返回
    /// 已提交时返回原上传结果；未提交为空。
    /// # 错误
    /// 绑定失效或同号不同文件内容时拒绝。
    pub async fn asset_upload_replay(
        &self,
        request_id: &str,
        expected_version: u64,
        data: &FileAssetData,
        key: &str,
        actor: &PortalActor,
    ) -> Result<Option<PortalAssetView>> {
        let current = self.session_validate(actor, &mut NoTransaction).await?;
        current.require_write()?;
        self.catalog_portal().detail(request_id, &current.supplier_id, &mut NoTransaction).await?;
        let command = scoped_command(
            &actor.account_id,
            &actor.supplier_id,
            "supplier_portal.asset_register",
            key,
            &asset_payload(request_id, expected_version, data),
        )?;
        PortalOfferingService::new(self.db.clone())
            .command_result(&command, &mut NoTransaction)
            .await?
            .map(|v| serde_json::from_value(v).map_err(|e| Error::Internal(e.to_string())))
            .transpose()
    }

    /// 在读取文件正文前证明原上传命令可以继续预检或查证。
    /// # 参数
    /// 原申请、原版本及原操作号，绑定身份和当前读取执行器。
    /// # 返回
    /// 原命令已有回执时允许后续完整指纹核对；未提交时检查原草稿版本。
    /// # 错误
    /// 越界、无写角色或未提交命令的草稿版本变化时拒绝。
    pub async fn asset_precheck_command(
        &self,
        request_id: &str,
        expected_version: u64,
        key: &str,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.session_validate(actor, executor).await?;
        current.require_write()?;
        self.catalog_portal().detail(request_id, &current.supplier_id, executor).await?;
        let command = scoped_command(
            &actor.account_id,
            &actor.supplier_id,
            "supplier_portal.asset_register",
            key,
            &json!({}),
        )?;
        if self
            .db
            .portal_command_receipts()
            .find_by_id_including_deleted(command.id(), executor)
            .await?
            .is_some()
        {
            return Ok(());
        }
        self.asset_precheck(request_id, expected_version, &current, executor).await
    }

    /// 在外部对象上传前证明申请归属、可编辑状态和原版本。
    /// # 参数
    /// 当前申请、预期版本、门户身份与读取执行器。
    /// # 返回
    /// 返回可上传资格，不执行任何文件或草稿写入。
    /// # 错误
    /// 越界、只读、已提交或版本失效时拒绝。
    pub async fn asset_precheck(
        &self,
        request_id: &str,
        expected_version: u64,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.session_validate(actor, executor).await?;
        current.require_write()?;
        let mut draft = self.catalog_portal().detail(request_id, &current.supplier_id, executor).await?;
        draft.update(expected_version, draft.draft.clone())?;
        Ok(())
    }

    /// 只允许下载当前供应商申请或其历史原稿明确引用的文件。
    /// # 参数
    /// 申请、文件、当前门户身份及读取执行器。
    /// # 返回
    /// 返回供协议层读取字节的受控文件事实。
    /// # 错误
    /// 来源不匹配、越界、过期或被销毁时统一拒绝。
    pub async fn asset_access(
        &self,
        request_id: &str,
        file_id: &str,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<FileAsset> {
        let current = self.session_validate(actor, executor).await?;
        let draft = self.catalog_portal().detail(request_id, &current.supplier_id, executor).await?;
        let referenced = references(&draft.draft, file_id)
            || draft.submissions.iter().any(|s| references(&s.input, file_id));
        if !referenced {
            return Err(Error::NotFound("附件不存在或无权访问".into()));
        }
        let asset = self
            .db
            .file_assets()
            .find_by_id(file_id, executor)
            .await?
            .filter(|a| a.is_usable_at(Instant::now()))
            .ok_or_else(|| Error::NotFound("附件不存在或无权访问".into()))?;
        Ok(asset)
    }

    pub(super) async fn validate_assets(
        &self,
        supplier_id: &str,
        request_id: Option<&str>,
        input: &NewProductInput,
        permanent: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let assets = input
            .image_asset_ids
            .iter()
            .chain(input.file_asset_ids.iter())
            .chain(input.skus.iter().filter_map(|row| row.image_asset_id.as_ref()))
            .collect::<HashSet<_>>();
        if assets.is_empty() {
            return Ok(());
        }
        let request_id =
            request_id.ok_or_else(|| Error::ValidationError("请先创建提报单，再上传关联素材".into()))?;
        let source = self.catalog_portal().detail(request_id, supplier_id, executor).await?;
        let owners: HashSet<_> = PortalIdentityService::new(self.db.clone())
            .account_list(supplier_id, executor)
            .await?
            .into_iter()
            .map(|a| a.account_id)
            .collect();
        for id in assets {
            if !references(&source.draft, id) && !source.submissions.iter().any(|s| references(&s.input, id))
            {
                return Err(Error::NotFound("附件未关联当前提报单".into()));
            }
            let mut asset = self
                .db
                .file_assets()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("附件不存在或无权访问".into()))?;
            if !owners.contains(&asset.created_by) || !asset.is_usable_at(Instant::now()) {
                return Err(Error::Forbidden("附件归属、保留状态或完整性无效".into()));
            }
            let kind = PortalAssetKind::from_content_type(&asset.content_type)?;
            kind.ensure_slot(
                input.image_asset_ids.contains(id)
                    || input.skus.iter().any(|sku| sku.image_asset_id.as_ref() == Some(id)),
            )?;
            if permanent && (asset.retention_class != RetentionClass::LongTerm || asset.expires_at.is_some())
            {
                asset.retention_class = RetentionClass::LongTerm;
                asset.expires_at = None;
                self.db.file_assets().update(&mut asset, executor).await?;
            }
        }
        Ok(())
    }
}

fn references(input: &NewProductInput, file_id: &str) -> bool {
    input.image_asset_ids.iter().chain(input.file_asset_ids.iter()).any(|id| id == file_id)
        || input.skus.iter().any(|row| row.image_asset_id.as_deref() == Some(file_id))
}
fn asset_payload(request_id: &str, version: u64, data: &FileAssetData) -> serde_json::Value {
    json!({"request_id":request_id,"expected_version":version,"content_type":data.content_type,"byte_size":data.byte_size,"content_hmac":data.content_hmac})
}

#[cfg(test)]
mod tests {
    use erp_support::entity::file_asset::{ContentHmac, SensitivityClass};

    use super::*;
    fn data(key: &str) -> FileAssetData {
        FileAssetData {
            storage_object_key: key.into(),
            file_name: "supplier-image.png".into(),
            content_type: "image/png".into(),
            byte_size: 24,
            content_hmac: ContentHmac::parse("ab".repeat(32)).unwrap(),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
            created_by: "external".into(),
        }
    }
    #[test]
    fn upload_fingerprint_ignores_random_object_keys_but_includes_file_content_and_target_version() {
        assert_eq!(
            asset_payload("draft", 1, &data("first-key")),
            asset_payload("draft", 1, &data("retry-key"))
        );
        assert_ne!(asset_payload("draft", 1, &data("key")), asset_payload("draft", 2, &data("key")));
        let mut changed = data("key");
        changed.content_hmac = ContentHmac::parse("cd".repeat(32)).unwrap();
        assert_ne!(asset_payload("draft", 1, &data("key")), asset_payload("draft", 1, &changed));
    }
    #[test]
    fn trusted_image_preparation_rejects_non_image_oversized_and_invalid_dimensions() {
        assert!(PreparedPortalAsset::from_decoded_image(data("key"), 100, 100).is_ok());
        assert!(PreparedPortalAsset::from_decoded_image(data("key"), 0, 100).is_err());
        assert!(PreparedPortalAsset::from_decoded_image(data("key"), 8193, 100).is_err());
        let mut pdf = data("key");
        pdf.content_type = "application/pdf".into();
        assert!(PreparedPortalAsset::from_decoded_image(pdf.clone(), 100, 100).is_err());
        assert!(PreparedPortalAsset::from_checked_pdf(pdf.clone(), 1).is_ok());
        assert!(PreparedPortalAsset::from_checked_pdf(pdf.clone(), 0).is_err());
        assert!(PreparedPortalAsset::from_checked_pdf(pdf, 501).is_err());
        assert!(PreparedPortalAsset::from_checked_pdf(data("key"), 1).is_err());
        let mut large = data("key");
        large.byte_size = 5 * 1024 * 1024 + 1;
        assert!(PreparedPortalAsset::from_decoded_image(large, 100, 100).is_err());
    }

    #[test]
    fn portal_kind_whitelist_keeps_pdf_out_of_public_product_images() {
        assert_eq!(
            PortalAssetKind::from_content_type("application/pdf").unwrap(),
            PortalAssetKind::Attachment
        );
        assert_eq!(serde_json::to_string(&PortalAssetKind::Attachment).unwrap(), "\"DOCUMENT\"");
        assert_eq!(
            serde_json::from_str::<PortalAssetKind>("\"DOCUMENT\"").unwrap(),
            PortalAssetKind::Attachment
        );
        let document = PortalAssetKind::from_content_type("application/pdf").unwrap();
        assert!(document.ensure_slot(false).is_ok());
        assert!(document.ensure_slot(true).is_err());
        for mime in ["image/jpeg", "image/png", "image/webp"] {
            assert_eq!(PortalAssetKind::from_content_type(mime).unwrap(), PortalAssetKind::Image);
            assert!(PortalAssetKind::from_content_type(mime).unwrap().ensure_slot(true).is_ok());
        }
        for mime in ["application/javascript", "image/svg+xml", "application/octet-stream"] {
            assert!(PortalAssetKind::from_content_type(mime).is_err());
        }
    }
}
