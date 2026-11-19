//! 测试专用失败映射，不生成可执行的查询条件。

use mongodb::bson::Bson;

use crate::entity::authorization::{AccessContext, AccessFact, MappingFacts, QueryModel, RelationKey};
use crate::ports::{AccessMapping, QueryFragment, RelationMapping};
use crate::{Error, Result};

/// 失败映射替身，只提供静态描述以进入身份取证，不提供可执行授权条件。
pub(crate) struct UnavailableMapping {
    /// 查询接入所需的静态模型描述，不代表可读取此模型。
    pub(crate) model: QueryModel,
}

impl AccessMapping for UnavailableMapping {
    // 暴露静态描述，使身份取证可以先于查询编译运行。
    fn model(&self) -> &QueryModel {
        &self.model
    }

    // 拒绝生成主键，避免将失败替身用于关联查询。
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Err(mapping_unavailable())
    }

    // 拒绝生成有效记录条件，不把不可用映射解释成全量读取。
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Err(mapping_unavailable())
    }

    // 拒绝生成事实谓词，供测试定位映射转换错误。
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        Err(mapping_unavailable())
    }

    // 拒绝解析来源，不提供可执行的子查询。
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        Err(mapping_unavailable())
    }
}

// 构造可与身份设施故障区分的测试错误，便于判断实际失败阶段。
fn mapping_unavailable() -> Error {
    Error::Internal("测试查询映射不可执行".into())
}
