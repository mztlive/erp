//! 测试专用失败映射，不生成可执行的查询条件。

use mongodb::bson::Bson;

use crate::entity::authorization::{AccessContext, AccessFact, MappingFacts, QueryModel, RelationKey};
use crate::ports::{AccessMapping, QueryFragment, RelationMapping};
use crate::{Error, Result};

/// 只允许query读取静态描述并进入身份取证，不提供任何可执行授权条件。
pub(crate) struct UnavailableMapping {
    pub(crate) model: QueryModel,
}

impl AccessMapping for UnavailableMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }

    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Err(mapping_unavailable())
    }

    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Err(mapping_unavailable())
    }

    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        Err(mapping_unavailable())
    }

    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        Err(mapping_unavailable())
    }
}

// 测试替身为 Mapping 返回与身份设施不同的错误，便于断言请求实际在哪层失败。
fn mapping_unavailable() -> Error {
    Error::Internal("测试查询映射不可执行".into())
}
