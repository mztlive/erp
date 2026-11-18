//! 来源条件只供固定身份、版本及责任投影，不能转换为领域完整对象凭证。
use std::sync::Arc;

use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::{QueryModel, RelationKey};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, aggregate};
use serde::Deserialize;

use crate::{Error, Result};

/// 同一根授权固定来源的内部查询；字段私有，不能构造通用领域 Scope。
#[derive(Clone)]
pub(crate) struct SourceQuery {
    root: Arc<CompiledAccess>,
    relation: RelationKey,
    model: QueryModel,
}

/// 来源身份与业务版本固定投影；不包含订单内容或金额。
#[derive(Debug, Deserialize)]
pub(crate) struct SourceVersion {
    pub id: String,
    pub version: u64,
}

/// 资金来源所需的销售责任投影；不包含销售商品明细或金额。
#[derive(Debug, Deserialize)]
pub(crate) struct SalesSourceFact {
    pub id: String,
    pub version: u64,
    pub owner_user_id: String,
    pub business_org_unit_id: String,
    pub document_no: String,
}

impl SourceQuery {
    /// 绑定根凭证中已有的固定来源，不生成新的授权凭证。
    /// # 参数
    /// root 为本次根凭证，relation 为服务端固定关系。
    /// # 返回
    /// 该入口提供来源时返回其内部查询，否则返回 None。
    /// # 错误
    /// 固定关系标识非法时拒绝。
    pub(crate) fn new(root: Arc<CompiledAccess>, relation: &str) -> Result<Option<Self>> {
        let relation = RelationKey::parse(relation)?;
        let Ok(model) = root.source_model(&relation).cloned() else {
            return Ok(None);
        };
        Ok(Some(Self { root, relation, model }))
    }

    /// 核验当前阶段及期限，不结束根凭证的读取阶段。
    /// # 参数
    /// executor 为原读取执行器。
    /// # 返回
    /// 根凭证仍有效时成功。
    /// # 错误
    /// 跨阶段、到期或事务失效拒绝。
    pub(crate) async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.root.ensure_current(executor)?;
        Ok(())
    }

    /// 返回原根授权内固定来源条件，仅用于本模块的投影查询。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原根绑定关系的数据库阶段。
    /// # 错误
    /// 构造已检查来源存在，凭证内关系不可变。
    pub(crate) fn stages(&self) -> &[Document] {
        self.root.source_stages(&self.relation).expect("SourceQuery构造已校验固定来源，根凭证不可变")
    }

    /// 只读取本批实际来源的身份，不交付完整订单对象。
    /// # 参数
    /// db 为数据库，ids 为本批引用，executor 为原执行器。
    /// # 返回
    /// 本批授权身份集合。
    /// # 错误
    /// 边界失效、必要版本缺失或查询失败时拒绝。
    pub(crate) async fn ids_in(
        &self,
        db: &Database,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(self.versions_in(db, ids, executor).await?.into_iter().map(|row| row.id).collect())
    }

    /// 使用固定字段读取身份和版本，不开放调用方投影选择。
    /// # 参数
    /// db 为数据库，ids 为实际来源，executor 为原执行器。
    /// # 返回
    /// 固定版本投影，按身份排序。
    /// # 错误
    /// 边界失效、版本缺失或查询失败时拒绝。
    pub(crate) async fn versions_in(
        &self,
        db: &Database,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SourceVersion>> {
        self.validate(executor).await?;
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut pipeline = vec![doc! {"$match": {&self.model.id_field: {"$in": ids}}}];
        pipeline.extend_from_slice(self.stages());
        pipeline.extend([
            doc! {"$project": {"_id":0, "id":format!("${}",self.model.id_field), "version":1}},
            doc! {"$sort":{"id":1}},
        ]);
        let result =
            aggregate::rows(&db.collection::<SourceVersion>(&self.model.collection), pipeline, executor)
                .await?;

        self.validate(executor).await?;
        Ok(result)
    }

    /// 读取真实销售来源所需的最小责任事实。
    /// # 参数
    /// db 为数据库，id 为实际销售引用，executor 为原执行器。
    /// # 返回
    /// 当前可读销售的身份、版本、单号及责任；不存在或无权时为 None。
    /// # 错误
    /// 来源资源错配、阶段失效或必要字段缺失时拒绝。
    pub(crate) async fn sales_fact(
        &self,
        db: &Database,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSourceFact>> {
        if self.model.resource != "sales_order" {
            return Err(Error::ValidationError("资金销售来源模型错配".into()));
        }
        self.validate(executor).await?;

        let mut pipeline = vec![doc! {"$match": {&self.model.id_field:id}}];
        pipeline.extend_from_slice(self.stages());
        pipeline.extend([sales_fact_projection(), doc! {"$limit":1}]);
        let result =
            aggregate::rows(&db.collection::<SalesSourceFact>(&self.model.collection), pipeline, executor)
                .await?
                .into_iter()
                .next();

        self.validate(executor).await?;
        Ok(result)
    }
}

/// 固定投影只开放资金关联需要的责任元数据。
fn sales_fact_projection() -> Document {
    doc! {"$project": {"_id":0, "id":1, "version":1, "owner_user_id":"$sales_owner_user_id", "business_org_unit_id":1, "document_no":"$order_no"}}
}

#[cfg(test)]
mod tests {
    use mongodb::bson::deserialize_from_document;

    use super::*;

    #[test]
    fn sales_fact_decodes_only_metadata_and_requires_its_business_version() {
        let row = doc! {"id":"sale", "version":2_i64,"owner_user_id":"sales", "business_org_unit_id":"org", "document_no":"SO1"};
        let fact = deserialize_from_document::<SalesSourceFact>(row.clone()).unwrap();
        assert_eq!((fact.id.as_str(), fact.version, fact.document_no.as_str()), ("sale", 2, "SO1"));

        let mut missing = row;
        missing.remove("version");
        assert!(deserialize_from_document::<SalesSourceFact>(missing).is_err());
    }
}
