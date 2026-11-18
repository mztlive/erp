use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};
use persistence_core::Executor;

use super::{QueryFragment, RelationMapping};
use crate::Result;
use crate::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, ObjectFacts, QueryModel, RelationKey, ResourceTargets,
};

/// 业务字段与关系映射。所有组合规则由身份域编译器执行。
pub trait AccessMapping: Send + Sync {
    /// 提供固定的根资源、集合和行身份描述，供启动登记及查询编译使用，不访问数据库。
    fn model(&self) -> &QueryModel;

    /// 声明完整且不变的来源关系和对应 Mapping，供 Builder 在启动时核对固定规则。不读动态事实；无来源返回空映射，漏登记规则依赖会被拒绝。
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        BTreeMap::new()
    }

    /// 声明授权过滤之前执行的固定关联准备，例如 `lookup`、`set` 或 `addFields`。
    /// 阶段不得依赖请求或授权结果，也不得过滤、展开或以其他方式改变根记录集合。
    fn stages(&self) -> Vec<Document> {
        Vec::new()
    }

    /// 按已冻结的授权时点计算静态失效边界，例如自然日归属的下一日边界。无时间约束返回 `None`；此步不加载动态数据。
    fn valid_until(&self, _context: &AccessContext) -> Option<Instant> {
        None
    }

    /// 为记录授权和来源关联提供当前行的 MongoDB 标量主键表达式，可用字段、转换或计算表达式。动态模型只使用本次准备事实；无法表达资源身份时拒绝。
    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson>;

    /// 只在条件转换依赖查询外动态数据时提供准备端口。静态 Mapping 返回 `None`，不制造空事实或假版本。
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        None
    }

    /// 为根查询及来源关联限定实际存在且未删除的记录。无软删除的专用模型可返回空条件；无法确认记录集合时拒绝，不能以业务状态替代对象授权。
    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment>;

    /// 编译授权规则中的单个业务事实，返回根条件或关联存在性条件。用本次身份时点及准备事实解释字段，不重组上层与或规则；不支持的维度须拒绝。
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment>;

    /// 给编译器提供已登记关系的完整来源 ID 表达式及对应 Mapping，不先剔除无权或缺失引用。关系未登记或无法证明完整性时拒绝。
    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<RelationMapping>;
}

/// 只有查询外读取动态数据的映射接入本能力；准备结果属于单次调用。
#[async_trait]
pub trait MappingPreparation: Send + Sync {
    /// 查询编译需要外部动态条件时，沿原执行器读取实际使用的事实、可靠修订号及最早到期时点。读取失败原样传播；非事务读取不能在缺少可靠修订号时交付结果。
    async fn prepare(&self, context: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts>;
}

/// 拥有领域提供当前对象事实；不得读取其他领域的集合。
#[async_trait]
pub trait AccessFactsPort: Send + Sync {
    /// 对象授权时沿调用方执行器读取完整根集合及真实来源，并按冻结身份及时点解释事实。缺失根对象不能用虚构对象补齐，读取与持久化错误原样传播。
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>>;
}
