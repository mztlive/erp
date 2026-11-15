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
    /// 取得固定查询模型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 根资源、集合和记录身份。
    /// # 错误
    /// 不执行 I/O。
    fn model(&self) -> &QueryModel;

    /// 描述全部固定来源映射，供启动校验；不读取动态准备数据。
    /// # 参数
    /// 无。
    /// # 返回
    /// 关系标识及对应的不可变 Mapping；没有来源时为空。
    /// # 错误
    /// 无；缺少规则所需关系由 Builder 拒绝。
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        BTreeMap::new()
    }

    /// 描述不改变根记录集合的固定关联准备。
    /// # 参数
    /// 无；不得依赖请求状态或授权结果。
    /// # 返回
    /// 完整关联的 lookup、set 或 addFields 阶段，先于授权过滤执行。
    /// # 错误
    /// 非只读阶段、根过滤或展开在启动时拒绝。
    fn stages(&self) -> Vec<Document> {
        Vec::new()
    }

    /// 返回静态时间解释的到期边界，不读取动态数据。
    /// # 参数
    /// context 为本次冻结的身份及时点。
    /// # 返回
    /// 例如自然日归属的下一日边界；无到期约束时为 None。
    /// # 错误
    /// 无；执行管道拒绝已经到期的边界。
    fn valid_until(&self, _context: &AccessContext) -> Option<Instant> {
        None
    }

    /// 返回当前记录的资源主键表达式。
    /// # 参数
    /// context 为授权上下文，prepared 为同次准备事实。
    /// # 返回
    /// MongoDB 标量表达式，支持字段、类型转换及计算主键。
    /// # 错误
    /// 无法表达该资源身份时返回接入错误。
    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson>;

    /// 取得查询外动态准备能力；静态映射无需实现。
    /// # 参数
    /// 无。
    /// # 返回
    /// 静态映射返回 None，不制造空事实或 Mapping 版本。
    /// # 错误
    /// 无；动态准备错误由 MappingPreparation 返回。
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        None
    }

    /// 限定模型中实际存在且未删除的记录，根查询与来源关联均应用。
    /// # 参数
    /// context 为本次身份与时点；prepared 为同次准备事实。
    /// # 返回
    /// 行存在性约束；没有软删除的专用模型可返回空字段条件。
    /// # 错误
    /// 无法确定有效记录集合时拒绝，禁止用对象状态替代权限检查。
    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment>;

    /// 翻译一个业务事实，不能改变上层与或关系。
    /// # 参数
    /// fact 为待翻译语义；context 和 prepared 为同次准备事实。
    /// # 返回
    /// 根条件或存在性查询。
    /// # 错误
    /// 未支持的事实必须拒绝。
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment>;

    /// 提供完整来源关系，不筛除无权或缺失的引用。
    /// # 参数
    /// relation 为登记关系；context 和 prepared 为同次准备事实。
    /// # 返回
    /// 实际引用 ID 表达式和来源映射。
    /// # 错误
    /// 未登记关系或无法证明完整性时拒绝。
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
    /// 经原执行器读取用于本次条件转换的完整动态数据。
    /// # 参数
    /// context 为固定身份和时点；executor 为原读取阶段。
    /// # 返回
    /// 数据、覆盖相应写入的真实修订号及最早到期边界。
    /// # 错误
    /// 读取失败原样传播；非事务缺少可靠修订号时由执行管道拒绝。
    async fn prepare(&self, context: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts>;
}

/// 拥有领域提供当前对象事实；不得读取其他领域的集合。
#[async_trait]
pub trait AccessFactsPort: Send + Sync {
    /// 读取本次根目标和完整来源关系。
    /// # 参数
    /// context 为身份及解释时点；ids 为完整根集合；executor 为原执行器。
    /// # 返回
    /// 实际读取的对象，缺失对象不能用虚构空对象替代。
    /// # 错误
    /// 持久化及事实读取错误原样传播。
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>>;
}
