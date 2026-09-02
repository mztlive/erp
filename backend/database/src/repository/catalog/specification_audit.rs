//! SKU 规格签名审计：扫描非规范 `specification_signature`，不参与销售列表热路径。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use serde::Deserialize;

use entities::catalog::parse_specification_signature;

use super::shared::SKUS;
use super::CatalogRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 审计投影：只读取稳定主键与规格签名。
#[derive(Debug, Deserialize)]
struct SpecificationSignatureAuditRow {
    id: String,
    #[serde(default)]
    specification_signature: String,
}

impl<'a> CatalogRepository<'a> {
    /// 审计未删除 SKU 中无法通过严格解析的规格签名。
    ///
    /// 本方法只供迁移/验收扫描，不用于公司商品池列表。命中为零时列表读取的
    /// 历史兼容分支可视为 N/A；存在命中时必须先迁移为规范签名，再把列表改为
    /// 严格失败关闭。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
    ///
    /// # 返回
    /// 返回非规范签名的 SKU 主键；全部规范时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn noncanonical_specification_signature_sku_ids(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let rows = mongo_ops::find_many(
            &self.db.collection::<SpecificationSignatureAuditRow>(SKUS),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::builder()
                .projection(doc! { "id": 1, "specification_signature": 1 })
                .build(),
            executor,
        )
        .await?;
        Ok(rows
            .into_iter()
            .filter(|row| parse_specification_signature(&row.specification_signature).is_err())
            .map(|row| row.id)
            .collect())
    }
}
