//! 单位代码、名称和符号的有界精确查询。
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use super::CatalogRepository;
use crate::Result;
use crate::entity::catalog::UnitOfMeasure;
use crate::repository::CatalogExt;

impl CatalogRepository<'_> {
    /// 按冻结采购单位代码读取名称，不接受模糊名称替代代码。同代码的单位或 None；历史停用单位仍可用于只读展示。
    ///
    /// `code` 为采购修订保存的单位代码，`executor` 为调用方执行器。
    pub async fn unit_by_code(
        &self,
        code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<UnitOfMeasure>> {
        Ok(self.db.unit_of_measures().find_one_by_field("unit_code", code, executor).await?)
    }

    /// 查找来源单位对应的全部候选，保留停用和删除记录以拒绝重复建档。至多两个候选；两个即足以确定歧义。
    ///
    /// `name` 为单位文本，`executor` 为调用方执行器。
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
