//! 供应商目录是读取观察记录，不建立公司商品、供给或销售价格。
use std::cmp::Ordering;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dto::supplier_catalog::{
    CatalogIdentityReviewView, CatalogSyncMode, ReviewCatalogIdentityRequest,
};
use crate::ports::connector::common::{Snapshot, SourceRevision};
pub use crate::ports::connector::offer::{CatalogAvailability, CatalogImage};
use crate::ports::connector::offer::{Offer, OfferDetails};
use crate::{Error, Result};

/// 一个连接内一个外部订货规格的最近观察；匹配只由人工操作改变。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SupplierCatalogItem {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub supplier_id: String,
    pub product_id: String,
    pub spec_id: String,
    pub name: String,
    pub specification: String,
    pub unit: Option<String>,
    pub purchase_price: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub category_name: Option<String>,
    #[serde(default)]
    pub brand_id: Option<String>,
    #[serde(default)]
    pub brand_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub images: Option<Vec<CatalogImage>>,
    #[serde(default)]
    pub detail_images: Option<Vec<CatalogImage>>,
    #[serde(default)]
    pub availability: CatalogAvailability,
    #[serde(default)]
    pub available_quantity: Option<String>,
    #[serde(default)]
    pub quantity_unbounded: bool,
    #[serde(default)]
    pub availability_valid_until: Option<Instant>,
    #[serde(default)]
    pub source_revision: SourceRevision,
    #[serde(default)]
    pub source_fingerprint: String,
    pub observed_at: Instant,
    pub source_updated_at: Option<Instant>,
    pub job_id: String,
    pub sku_id: Option<String>,
    #[serde(default)]
    pub sku_no: Option<String>,
    #[serde(default)]
    pub company_product_id: Option<String>,
    #[serde(default)]
    pub offering_id: Option<String>,
    #[serde(default)]
    pub requires_identity_review: bool,
}
impl SupplierCatalogItem {
    /// 合并最新观察时保留同身份记录的人工匹配与持久化版本。
    /// # 参数
    /// `previous` 为同一来源身份的既有目录记录。
    /// # 返回
    /// 将既有身份和人工匹配复制到当前观察。
    /// # 错误
    /// 来源身份不相同则拒绝，禁止跨连接沿用匹配。
    pub fn retain_match(&mut self, previous: Self) -> Result<()> {
        if self.base.id != previous.base.id
            || self.connection_id != previous.connection_id
            || self.product_id != previous.product_id
            || self.spec_id != previous.spec_id
        {
            return Err(Error::ConflictError("供应商商品身份不一致".into()));
        }
        self.base = previous.base;
        self.requires_identity_review = previous.requires_identity_review
            || (previous.offering_id.is_some()
                && (self.specification != previous.specification || self.unit != previous.unit));
        self.sku_id = previous.sku_id;
        self.sku_no = previous.sku_no;
        self.company_product_id = previous.company_product_id;
        self.offering_id = previous.offering_id;
        self.description = self.description.take().or(previous.description);
        self.images = self.images.take().or(previous.images);
        self.detail_images = self.detail_images.take().or(previous.detail_images);
        self.category_id = self.category_id.take().or(previous.category_id);
        self.category_name = self.category_name.take().or(previous.category_name);
        self.brand_id = self.brand_id.take().or(previous.brand_id);
        self.brand_name = self.brand_name.take().or(previous.brand_name);
        self.refresh_fingerprint()?;
        Ok(())
    }

    /// 将人工选择的、已由调用方核验的公司 SKU 保存为匹配。
    /// # 参数
    /// 预期目录版本与当前授权公司 SKU 的身份、业务编号和商品身份。
    /// # 返回
    /// 匹配变化为 true；重复确认同一 SKU 为 false。
    /// # 错误
    /// 变更匹配时目录版本冲突则拒绝。
    pub fn match_sku(
        &mut self,
        expected_version: u64,
        sku_id: String,
        sku_no: String,
        product_id: String,
    ) -> Result<bool> {
        if self.sku_id.as_ref() == Some(&sku_id) {
            return Ok(false);
        }
        if self.offering_id.is_some() {
            return Err(Error::ConflictError("已选品商品须通过专门操作重新关联，不允许直接更换SKU".into()));
        }
        if self.base.version != expected_version {
            return Err(Error::ConflictError("商品目录已变化，请刷新后重新匹配".into()));
        }
        self.sku_id = Some(sku_id);
        self.sku_no = Some(sku_no);
        self.company_product_id = Some(product_id);
        Ok(true)
    }

    /// 将来源目录观察转为待选品记录；不声称已经取得公司销售资格。
    /// # 参数
    /// 连接、供应商和任务身份由服务器提供，`snapshot` 来自协议适配器。
    /// # 返回
    /// 按连接、商品和规格确定稳定身份的目录记录。
    /// # 错误
    /// 身份缺失、名称为空或来源资料无法生成摘要时拒绝。
    pub fn observed(connection: &str, supplier: &str, job: &str, snapshot: Snapshot<Offer>) -> Result<Self> {
        let offer = snapshot.value;
        let product = offer
            .sku
            .product_id
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| Error::ValidationError("供应商商品编号缺失".into()))?;
        if offer.sku.spec_id.trim().is_empty() || offer.name.trim().is_empty() {
            return Err(Error::ValidationError("供应商规格编号或商品名称缺失".into()));
        }
        let details = offer.details;
        let mut item = Self {
            base: BaseModel::new(format!(
                "sc-{}",
                catalog_identity(&[connection, &product, &offer.sku.spec_id])
            )),
            connection_id: connection.into(),
            supplier_id: supplier.into(),
            product_id: product,
            spec_id: offer.sku.spec_id,
            name: offer.name,
            specification: offer.specification,
            unit: offer.unit,
            purchase_price: offer
                .quote
                .and_then(|quote| quote.dropship)
                .map(|price| price.unit_price.to_string()),
            category_id: details.category_id,
            category_name: details.category_name,
            brand_id: details.brand_id,
            brand_name: details.brand_name,
            description: details.description,
            images: details.images,
            detail_images: details.detail_images,
            availability: details.availability,
            available_quantity: details.available_quantity,
            quantity_unbounded: details.quantity_unbounded,
            availability_valid_until: None,
            source_revision: snapshot.stamp.revision,
            source_fingerprint: String::new(),
            observed_at: snapshot.stamp.observed_at,
            source_updated_at: snapshot.stamp.changed_at,
            job_id: job.into(),
            sku_id: None,
            sku_no: None,
            company_product_id: None,
            offering_id: None,
            requires_identity_review: false,
        };
        item.refresh_fingerprint()?;
        Ok(item)
    }

    /// 记录明确选品结果；既有不同供给不可覆写。
    /// # 参数
    /// 预期目录版本和已在同事务建立的公司 SKU、商品及供给身份。
    /// # 返回
    /// 保存关联；同一正式选品结果保持幂等。
    /// # 错误
    /// 目录版本变化、空身份或已有其他供给时拒绝。
    pub fn record_adoption(
        &mut self,
        expected_version: u64,
        sku_id: String,
        sku_no: String,
        product_id: String,
        offering_id: String,
    ) -> Result<()> {
        if [sku_id.as_str(), product_id.as_str(), offering_id.as_str()].iter().any(|id| id.trim().is_empty())
        {
            return Err(Error::ValidationError("选品关联身份不能为空".into()));
        }
        if let Some(existing) = &self.offering_id {
            if existing == &offering_id && self.sku_id.as_ref() == Some(&sku_id) {
                return Ok(());
            }
            return Err(Error::ConflictError("供应商商品已关联其他正式供给".into()));
        }
        if self.base.version != expected_version {
            return Err(Error::ConflictError("商品目录已变化，请重新预览选品".into()));
        }
        self.sku_id = Some(sku_id);
        self.sku_no = Some(sku_no);
        self.company_product_id = Some(product_id);
        self.offering_id = Some(offering_id);
        Ok(())
    }

    /// 计算连接内供应商商品规格的稳定目录身份。
    /// # 参数
    /// 已核验的连接、来源商品及规格编号。
    /// # 返回
    /// 与首次采集相同的目录主键。
    /// # 错误
    /// 不返回错误；输入合法性由协议边界负责。
    pub fn source_id(connection: &str, product: &str, spec: &str) -> String {
        format!("sc-{}", catalog_identity(&[connection, product, spec]))
    }

    /// 拒绝来源单位与公司已确认基础单位不同的身份恢复。
    /// # 参数
    /// 公司字典中同一个单位的代码、名称及符号。
    /// # 返回
    /// 来源未报告单位时由明确人工确认负责；已报告时须精确匹配一种字典表达。
    /// # 错误
    /// 单位不同则拒绝，不执行隐式数量换算。
    pub fn ensure_company_unit(&self, aliases: &[&str]) -> Result<()> {
        if let Some(source) = &self.unit
            && !aliases.iter().any(|alias| alias.trim().eq_ignore_ascii_case(source.trim()))
        {
            return Err(Error::BusinessLogicError(
                "来源单位与公司基础单位不同，须重新建立正确SKU和供给".into(),
            ));
        }
        Ok(())
    }

    /// 在调用方已核对公司 SKU 和供给后确认来源身份未实质变化。
    /// # 参数
    /// 人工看过的来源版本、资料与明确同一销售项确认。
    /// # 返回
    /// 解除来源身份核对阻断，不修改公司 SKU、单位及供给身份。
    /// # 错误
    /// 来源版本或内容变化、未明确确认、原因缺失及来源撤回时拒绝。
    pub fn confirm_identity_review(&mut self, request: &ReviewCatalogIdentityRequest) -> Result<()> {
        if self.base.version != request.expected_version
            || self.specification != request.source_specification
            || self.unit != request.source_unit
        {
            return Err(Error::ConflictError("来源规格或单位已变化，请重新核对".into()));
        }
        if !request.confirm_same_identity
            || request.reason.trim().is_empty()
            || request.reason.len() > 1000
            || request.idempotency_key.trim().is_empty()
            || request.idempotency_key.len() > 200
        {
            return Err(Error::ValidationError("须确认同一实际销售项并填写核对原因".into()));
        }
        if !self.requires_identity_review {
            return Err(Error::BusinessLogicError("当前来源没有待核对的身份变化".into()));
        }
        if self.offering_id.is_none() || self.availability == CatalogAvailability::Withdrawn {
            return Err(Error::BusinessLogicError("当前来源未建立供给或已撤回，不能恢复使用".into()));
        }
        self.requires_identity_review = false;
        Ok(())
    }

    /// 合并来源观察，拒绝可证明旧版本及同版本异载荷。
    /// # 参数
    /// `previous` 必须是同一连接和外部 SKU 的持久化记录。
    /// # 返回
    /// 内容应写入为 true；旧版本或重复观察为 false。
    /// # 错误
    /// 身份不符、同版本不同内容或无法序列化时拒绝。
    pub fn merge_observation(&mut self, previous: Self) -> Result<bool> {
        if self.base.id != previous.base.id
            || self.connection_id != previous.connection_id
            || self.product_id != previous.product_id
            || self.spec_id != previous.spec_id
        {
            return Err(Error::ConflictError("供应商商品身份不一致".into()));
        }
        let order = self.source_revision.compare(&previous.source_revision);
        let previous_fingerprint = previous.source_fingerprint.clone();
        if order == Some(Ordering::Less) {
            return Ok(false);
        }
        self.retain_match(previous)?;
        if self.source_fingerprint == previous_fingerprint {
            return Ok(false);
        }
        if order == Some(Ordering::Equal) {
            return Err(Error::ConflictError("供应商同一来源版本包含不同商品资料".into()));
        }
        Ok(true)
    }

    /// 保存供应商明确撤回；列表缺项不得调用此操作。
    /// # 参数
    /// 已核验的同对象来源版本、接收时间及任务身份。
    /// # 返回
    /// 是否需要持久化本次撤回。
    /// # 错误
    /// 指纹构造失败时返回错误。
    pub fn withdraw(&mut self, revision: SourceRevision, at: Instant, job: &str) -> Result<bool> {
        match revision.compare(&self.source_revision) {
            Some(Ordering::Less) => return Ok(false),
            Some(Ordering::Equal) if self.availability == CatalogAvailability::Withdrawn => return Ok(false),
            Some(Ordering::Equal) => {
                return Err(Error::ConflictError("供应商同一来源版本包含不同撤回事实".into()));
            },
            _ => {},
        }
        self.availability = CatalogAvailability::Withdrawn;
        self.available_quantity = None;
        self.quantity_unbounded = false;
        self.source_revision = revision;
        self.observed_at = at;
        self.job_id = job.to_owned();
        self.refresh_fingerprint()?;
        Ok(true)
    }

    /// 根据当前来源资料计算稳定内容摘要，不包含接收时间和公司匹配。
    /// # 参数
    /// 无；素材准备后使用当前图片内容指纹。
    /// # 返回
    /// 更新来源内容摘要。
    /// # 错误
    /// 内容无法序列化时返回错误。
    pub fn refresh_fingerprint(&mut self) -> Result<()> {
        let details = OfferDetails {
            category_id: self.category_id.clone(),
            category_name: self.category_name.clone(),
            brand_id: self.brand_id.clone(),
            brand_name: self.brand_name.clone(),
            description: self.description.clone(),
            images: self.images.clone(),
            detail_images: self.detail_images.clone(),
            availability: self.availability,
            available_quantity: self.available_quantity.clone(),
            quantity_unbounded: self.quantity_unbounded,
        };
        let bytes =
            serde_json::to_vec(&(&self.name, &self.specification, &self.unit, &self.purchase_price, details))
                .map_err(|_| Error::ValidationError("供应商商品资料无法生成版本指纹".into()))?;
        self.source_fingerprint = hex::encode(Sha256::digest(bytes));
        Ok(())
    }
}

/// 分页检查点与连接配置快照；与每页目录及任务进度同事务保存。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SupplierCatalogRun {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub technical_config_version: u64,
    pub cursor: Option<String>,
    pub pages: u32,
    pub observed_count: u64,
    pub finished: bool,
    #[serde(default)]
    pub sync_mode: CatalogSyncMode,
    #[serde(default)]
    pub scan_since: Option<Instant>,
    #[serde(default = "Instant::now")]
    pub scan_through: Instant,
}

/// 人工身份核对的永久命令回执；不得由展示审计替代。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogIdentityReviewReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub item_id: String,
    pub actor_id: String,
    pub fingerprint: String,
    pub reason: String,
    pub result: CatalogIdentityReviewView,
}
impl SupplierCatalogRun {
    /// 为任务冻结技术配置和初始游标。
    /// # 参数
    /// `job` 为任务 ID，`connection` 为已自动配置的连接。
    /// # 返回
    /// 尚未扫描的检查点。
    /// # 错误
    /// 不返回错误。
    pub fn new(job: &str, connection: &super::supplier_api::SupplierApiConnection) -> Self {
        Self {
            base: BaseModel::new(job.into()),
            connection_id: connection.base.id.clone(),
            technical_config_version: connection.technical_config_version,
            cursor: None,
            pages: 0,
            observed_count: 0,
            finished: false,
            sync_mode: CatalogSyncMode::Full,
            scan_since: None,
            scan_through: Instant::now(),
        }
    }
    /// 仅在一页全部保存后推进检查点；限制扫描次数并拒绝自循环游标。
    /// # 参数
    /// `next` 为供应商后续游标，`count` 为本页规格数。
    /// # 返回
    /// 更新页数和观察数。
    /// # 错误
    /// 重复游标、超过 10000 页或计数溢出时拒绝。
    pub fn advance(&mut self, next: Option<String>, count: u64) -> Result<()> {
        if self.finished
            || self.pages >= 10000
            || next.as_ref().is_some_and(|v| Some(v) == self.cursor.as_ref())
        {
            return Err(Error::BusinessLogicError("商品分页未前进或已超出同步上限，请联系管理员".into()));
        }
        self.observed_count = self
            .observed_count
            .checked_add(count)
            .ok_or_else(|| Error::BusinessLogicError("商品数量超出上限".into()))?;
        self.pages += 1;
        self.finished = next.is_none();
        self.cursor = next;
        Ok(())
    }
}

/// 长度分帧避免外部编号包含分隔符时发生身份碰撞。
fn catalog_identity(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.len().to_be_bytes());
        hash.update(part.as_bytes());
    }
    hex::encode(hash.finalize())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};

    use super::*;
    use crate::entity::supplier_api::{
        SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use crate::ports::connector::common::{SourceStamp, SupplierSku};
    fn observation() -> Snapshot<Offer> {
        Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(100),
            },
            value: Offer {
                sku: SupplierSku { product_id: Some("p1".into()), spec_id: "s1".into() },
                name: "蛋糕".into(),
                specification: "6寸".into(),
                unit: None,
                quote: None,
                details: OfferDetails::default(),
            },
        }
    }
    fn connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("c1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier"),
                connection_code: "DGSS".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap()
    }
    #[test]
    fn repeated_observations_keep_identity_without_inventing_price_time_or_sku() {
        let first = SupplierCatalogItem::observed("c1", "supplier", "j1", observation()).unwrap();
        let second = SupplierCatalogItem::observed("c1", "supplier", "j2", observation()).unwrap();
        assert_eq!(first.base.id, second.base.id);
        assert_ne!(
            first.base.id,
            SupplierCatalogItem::observed("c2", "supplier", "j1", observation()).unwrap().base.id
        );
        assert!(
            first.purchase_price.is_none()
                && first.unit.is_none()
                && first.sku_id.is_none()
                && first.source_updated_at.is_none()
        );
        assert_ne!(catalog_identity(&["a|b", "c"]), catalog_identity(&["a", "b|c"]));
    }
    #[test]
    fn matching_checks_version_and_resync_preserves_manual_match() {
        let mut prior = SupplierCatalogItem::observed("c", "s", "j1", observation()).unwrap();
        assert!(prior.match_sku(prior.base.version + 1, "sku".into(), "SKU-1".into(), "p".into()).is_err());
        assert!(prior.match_sku(prior.base.version, "sku".into(), "SKU-1".into(), "p".into()).unwrap());
        assert!(!prior.match_sku(prior.base.version + 1, "sku".into(), "SKU-1".into(), "p".into()).unwrap());
        let mut fresh = SupplierCatalogItem::observed("c", "s", "j2", observation()).unwrap();
        fresh.retain_match(prior.clone()).unwrap();
        assert_eq!(fresh.sku_id.as_deref(), Some("sku"));
        assert_eq!(fresh.job_id, "j2");
        let mut other = SupplierCatalogItem::observed("other", "s", "j2", observation()).unwrap();
        assert!(other.retain_match(prior).is_err());
        assert!(other.sku_id.is_none());
    }

    #[test]
    fn invalid_identity_is_rejected_and_source_revision_is_retained() {
        let mut source = observation();
        source.value.sku.product_id = None;
        assert!(SupplierCatalogItem::observed("c", "s", "j", source).is_err());
        let mut source = observation();
        source.value.sku.spec_id.clear();
        assert!(SupplierCatalogItem::observed("c", "s", "j", source).is_err());
        let mut source = observation();
        source.stamp.revision = SourceRevision::EqualityToken("v1".into());
        let item = SupplierCatalogItem::observed("c", "s", "j", source).unwrap();
        assert_eq!(item.source_revision, SourceRevision::EqualityToken("v1".into()));
    }
    #[test]
    fn page_checkpoint_counts_empty_pages_and_rejects_repeat_cursor() {
        let mut run = SupplierCatalogRun::new("job", &connection());
        run.advance(Some("2".into()), 3).unwrap();
        let previous = (run.cursor.clone(), run.observed_count, run.pages);
        assert!(run.advance(Some("2".into()), 7).is_err());
        assert_eq!((run.cursor.clone(), run.observed_count, run.pages), previous);
        run.advance(Some("3".into()), 0).unwrap();
        run.advance(None, 2).unwrap();
        assert!(run.finished);
        assert_eq!((run.pages, run.observed_count), (3, 5));
        assert!(run.advance(None, 0).is_err());
    }
    #[test]
    fn source_merge_preserves_adoption_and_omitted_media_and_blocks_identity_changes() {
        let mut source = observation();
        source.value.details.images = Some(vec![CatalogImage {
            url: "https://example.com/a.jpg".into(),
            content_fingerprint: Some("sha".into()),
            file_asset_id: Some("asset".into()),
        }]);
        let mut prior = SupplierCatalogItem::observed("c", "s", "j1", source).unwrap();
        prior
            .record_adoption(
                prior.base.version,
                "sku".into(),
                "SKU".into(),
                "product".into(),
                "offering".into(),
            )
            .unwrap();
        assert!(prior.match_sku(prior.base.version, "other-sku".into(), "O".into(), "other".into()).is_err());
        let mut fresh = SupplierCatalogItem::observed("c", "s", "j2", observation()).unwrap();
        fresh.retain_match(prior.clone()).unwrap();
        assert_eq!(fresh.offering_id.as_deref(), Some("offering"));
        assert_eq!(fresh.images, prior.images);
        assert!(!fresh.requires_identity_review);
        let mut changed = observation();
        changed.value.specification = "8寸".into();
        let mut changed = SupplierCatalogItem::observed("c", "s", "j3", changed).unwrap();
        changed.retain_match(fresh).unwrap();
        assert!(changed.requires_identity_review);
        let mut repeated = SupplierCatalogItem::observed("c", "s", "j4", observation()).unwrap();
        repeated.retain_match(changed).unwrap();
        assert!(repeated.requires_identity_review);
    }

    #[test]
    fn comparable_old_source_and_duplicate_are_ignored_and_same_version_conflict_fails() {
        let mut source = observation();
        source.stamp.revision = SourceRevision::Sequence { scope: "catalog".into(), value: 2 };
        let prior = SupplierCatalogItem::observed("c", "s", "j1", source.clone()).unwrap();
        let mut duplicate = SupplierCatalogItem::observed("c", "s", "j2", source.clone()).unwrap();
        assert!(!duplicate.merge_observation(prior.clone()).unwrap());
        source.value.name = "另一名称".into();
        let mut conflict = SupplierCatalogItem::observed("c", "s", "j3", source.clone()).unwrap();
        assert!(conflict.merge_observation(prior.clone()).is_err());
        source.stamp.revision = SourceRevision::Sequence { scope: "catalog".into(), value: 1 };
        let mut old = SupplierCatalogItem::observed("c", "s", "j4", source).unwrap();
        assert!(!old.merge_observation(prior.clone()).unwrap());
        assert_eq!(prior.name, "蛋糕");
    }

    #[test]
    fn explicit_withdrawal_uses_stable_identity_and_rejects_old_versions() {
        let mut source = observation();
        source.stamp.revision = SourceRevision::Sequence { scope: "catalog".into(), value: 2 };
        let mut item = SupplierCatalogItem::observed("c", "s", "j1", source).unwrap();
        assert_eq!(item.base.id, SupplierCatalogItem::source_id("c", "p1", "s1"));
        assert!(
            !item
                .withdraw(
                    SourceRevision::Sequence { scope: "catalog".into(), value: 1 },
                    Instant::from_unix_secs(101),
                    "j2"
                )
                .unwrap()
        );
        assert!(
            item.withdraw(
                SourceRevision::Sequence { scope: "catalog".into(), value: 3 },
                Instant::from_unix_secs(102),
                "j3"
            )
            .unwrap()
        );
        assert_eq!(item.availability, CatalogAvailability::Withdrawn);
        assert!(item.available_quantity.is_none());
        assert!(!item.quantity_unbounded);
    }
    #[test]
    fn identity_review_requires_explicit_same_item_confirmation_and_preserves_sku() {
        let mut item = SupplierCatalogItem::observed("c", "s", "j", observation()).unwrap();
        item.record_adoption(
            item.base.version,
            "sku".into(),
            "SKU".into(),
            "product".into(),
            "offering".into(),
        )
        .unwrap();
        item.requires_identity_review = true;
        let mut request = ReviewCatalogIdentityRequest {
            expected_version: item.base.version,
            expected_sku_revision_id: "revision".into(),
            expected_offering_version: 1,
            source_specification: item.specification.clone(),
            source_unit: item.unit.clone(),
            confirmed_base_unit_id: "unit".into(),
            confirm_same_identity: false,
            reason: "已与供应商核实包装与计量含义不变".into(),
            idempotency_key: "review".into(),
        };
        assert!(item.confirm_identity_review(&request).is_err());
        assert!(item.requires_identity_review);
        request.confirm_same_identity = true;
        request.expected_version += 1;
        assert!(item.confirm_identity_review(&request).is_err());
        request.expected_version = item.base.version;
        item.confirm_identity_review(&request).unwrap();
        assert!(!item.requires_identity_review);
        assert_eq!(item.sku_id.as_deref(), Some("sku"));
        assert_eq!(item.offering_id.as_deref(), Some("offering"));
        assert!(item.confirm_identity_review(&request).is_err());
    }

    #[test]
    fn identity_review_rejects_unit_conversion_and_withdrawn_source() {
        let mut item = SupplierCatalogItem::observed("c", "s", "j", observation()).unwrap();
        item.unit = Some("盒".into());
        assert!(item.ensure_company_unit(&["piece", "件", "件"]).is_err());
        item.ensure_company_unit(&["box", "盒", "盒"]).unwrap();
        item.record_adoption(
            item.base.version,
            "sku".into(),
            "SKU".into(),
            "product".into(),
            "offering".into(),
        )
        .unwrap();
        item.requires_identity_review = true;
        item.availability = CatalogAvailability::Withdrawn;
        let request = ReviewCatalogIdentityRequest {
            expected_version: item.base.version,
            expected_sku_revision_id: "revision".into(),
            expected_offering_version: 1,
            source_specification: item.specification.clone(),
            source_unit: item.unit.clone(),
            confirmed_base_unit_id: "box".into(),
            confirm_same_identity: true,
            reason: "说明".into(),
            idempotency_key: "review".into(),
        };
        assert!(item.confirm_identity_review(&request).is_err());
        assert!(item.requires_identity_review);
    }
}
