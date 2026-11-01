//! 导入预览持久化适配；Mongo TTL 日期不进入领域实体或传输合同。

use entity_core::{BaseModel, HasBaseModel};
use mongodb::Database;
use mongodb::bson::DateTime;
use persistence_core::{Error, Executor, Repository, Result};
use serde::{Deserialize, Serialize};

use super::extensions::SalesInquiryImportExt;
use crate::entity::sales_inquiry_import::InquiryImportPreview;

#[derive(Serialize, Deserialize)]
struct StoredInquiryImportPreview {
    #[serde(flatten)]
    preview: InquiryImportPreview,
    expires_at_date: DateTime,
}

impl StoredInquiryImportPreview {
    fn new(preview: &InquiryImportPreview) -> Result<Self> {
        let milliseconds =
            preview.expires_at.checked_mul(1000).ok_or(Error::EntityMetadataOutOfRange("预览到期时间"))?;
        Ok(Self { preview: preview.clone(), expires_at_date: DateTime::from_millis(milliseconds) })
    }
}

impl HasBaseModel for StoredInquiryImportPreview {
    fn base(&self) -> &BaseModel {
        &self.preview.base
    }
    fn base_mut(&mut self) -> &mut BaseModel {
        &mut self.preview.base
    }
}

/// 对领域调用方只暴露预览实体，隔离 TTL 的 BSON 存储形态。
pub struct InquiryImportPreviewRepository<'a> {
    inner: Repository<'a, StoredInquiryImportPreview>,
}

impl<'a> InquiryImportPreviewRepository<'a> {
    pub(crate) fn new(db: &'a Database) -> Self {
        Self { inner: Repository::new(db, Database::INQUIRY_IMPORT_PREVIEWS) }
    }

    /// 保存新的账号及询价专用预览，并写入 TTL 日期。
    /// # 参数
    /// `preview` 为领域预览，`executor` 为原用例执行器。
    /// # 返回
    /// 保存成功时返回空结果。
    /// # 错误
    /// 到期时刻越界、编码或数据库失败时返回存储错误。
    pub async fn create(&self, preview: &InquiryImportPreview, executor: &mut dyn Executor) -> Result<()> {
        self.inner.create(&StoredInquiryImportPreview::new(preview)?, executor).await
    }

    /// 读取未删除预览，去除存储专用日期字段。
    /// # 参数
    /// `id` 为预览身份，`executor` 为原用例执行器。
    /// # 返回
    /// 返回可选领域实体；业务到期及访问绑定由领域守卫校验。
    /// # 错误
    /// 数据库或解码失败时返回存储错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryImportPreview>> {
        Ok(self.inner.find_by_id(id, executor).await?.map(|stored| stored.preview))
    }

    /// 按原预览版本写入确认结果及 TTL 日期。
    /// # 参数
    /// `preview` 为已经通过领域守卫的实体，`executor` 为原确认事务。
    /// # 返回
    /// 写入成功后同步聚合元数据。
    /// # 错误
    /// CAS、时刻越界、编码或数据库失败时返回存储错误。
    pub async fn update(
        &self,
        preview: &mut InquiryImportPreview,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut stored = StoredInquiryImportPreview::new(preview)?;
        self.inner.update(&mut stored, executor).await?;
        preview.base = stored.preview.base;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson;
    use persistence_core::persisted_document;

    use super::*;
    use crate::entity::sales_inquiry::InquiryRequirementData;

    fn preview(expires_at: i64) -> InquiryImportPreview {
        InquiryImportPreview {
            base: BaseModel::new("preview".into()),
            inquiry_id: "inquiry".into(),
            actor_id: "actor".into(),
            creation_customer_id: None,
            expected_version: 1,
            input_fingerprint: "a".repeat(64),
            errors: vec![],
            merges: vec![],
            data: InquiryRequirementData::default(),
            expires_at,
            result: None,
        }
    }

    #[test]
    fn ttl_date_is_persisted_as_bson_date_without_entering_domain_wire_shape() {
        let stored = StoredInquiryImportPreview::new(&preview(1234)).unwrap();
        let document = persisted_document(&stored).unwrap();
        assert_eq!(document.get_datetime("expires_at_date").unwrap().timestamp_millis(), 1_234_000);
        let bytes = bson::serialize_to_vec(&stored).unwrap();
        let loaded: StoredInquiryImportPreview = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(loaded.preview.expires_at, 1234);
        let wire = serde_json::to_value(&loaded.preview).unwrap();
        assert!(wire.get("expires_at_date").is_none());
        assert_eq!(wire["inquiry_id"], "inquiry");
    }

    #[test]
    fn expiry_conversion_rejects_overflow() {
        assert!(StoredInquiryImportPreview::new(&preview(i64::MAX)).is_err());
    }
}
