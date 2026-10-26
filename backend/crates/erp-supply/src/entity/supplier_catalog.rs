//! 供应商目录是读取观察记录，不建立公司商品、供给或销售价格。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ports::connector::common::{Snapshot, SourceRevision};
use crate::ports::connector::offer::Offer;
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
    pub observed_at: Instant,
    pub source_updated_at: Option<Instant>,
    pub job_id: String,
    pub sku_id: Option<String>,
    #[serde(default)]
    pub sku_no: Option<String>,
    #[serde(default)]
    pub company_product_id: Option<String>,
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
        self.sku_id = previous.sku_id;
        self.sku_no = previous.sku_no;
        self.company_product_id = previous.company_product_id;
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
        if self.base.version != expected_version {
            return Err(Error::ConflictError("商品目录已变化，请刷新后重新匹配".into()));
        }
        self.sku_id = Some(sku_id);
        self.sku_no = Some(sku_no);
        self.company_product_id = Some(product_id);
        Ok(true)
    }

    /// 将无版本目录观察转为待匹配记录；不声称是正式价格或可供证明。
    /// # 参数
    /// 连接、供应商和任务身份由服务器提供，`snapshot` 来自协议适配器。
    /// # 返回
    /// 按连接、商品和规格确定稳定身份的目录记录。
    /// # 错误
    /// 身份缺失、名称为空或来源版本未受支持时拒绝。
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
        if snapshot.stamp.revision != SourceRevision::Unversioned {
            return Err(Error::ValidationError("该目录来源版本尚未支持".into()));
        }
        Ok(Self {
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
            observed_at: snapshot.stamp.observed_at,
            source_updated_at: snapshot.stamp.changed_at,
            job_id: job.into(),
            sku_id: None,
            sku_no: None,
            company_product_id: None,
        })
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
    fn invalid_identity_and_unsupported_revision_are_rejected() {
        let mut source = observation();
        source.value.sku.product_id = None;
        assert!(SupplierCatalogItem::observed("c", "s", "j", source).is_err());
        let mut source = observation();
        source.value.sku.spec_id.clear();
        assert!(SupplierCatalogItem::observed("c", "s", "j", source).is_err());
        let mut source = observation();
        source.stamp.revision = SourceRevision::EqualityToken("v1".into());
        assert!(SupplierCatalogItem::observed("c", "s", "j", source).is_err());
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
}
