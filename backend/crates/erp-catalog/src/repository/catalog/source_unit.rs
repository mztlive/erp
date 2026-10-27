//! 单位代码、名称和符号的有界精确查询。
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use super::CatalogRepository;
use crate::Result;
use crate::entity::catalog::UnitOfMeasure;
use crate::repository::CatalogExt;

impl CatalogRepository<'_> {
    /// 查找来源单位对应的全部候选，保留停用和删除记录以拒绝重复建档。
    /// # 参数
    /// `name` 为单位文本，`executor` 为调用方执行器。
    /// # 返回
    /// 至多两个候选；两个即足以确定歧义。
    /// # 错误
    /// 数据库查询失败时返回错误。
    pub async fn source_unit_candidates(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<UnitOfMeasure>> {
        let name = name.trim();
        let labels = vec![name.to_owned(), name.to_lowercase(), name.to_uppercase()];
        let codes = [labels.clone(), vec![format!("AUTO-{}", name.to_uppercase())]].concat();
        Ok(mongo_ops::find_many(
            &self.db.unit_of_measures().collection(),
            doc! {"$or": [{"name": {"$in": &labels}}, {"symbol": {"$in": &labels}}, {"unit_code": {"$in": codes}}]},
            FindOptions::builder().limit(2).build(),
            executor,
        ).await?)
    }
}
