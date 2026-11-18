//! 启动描述与实际 Mapping 的连接；运行时不得替换模型或来源实现。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};

use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, MappingFacts, QueryModel, QueryOutput, RelationKey, RelationMatch,
};
use crate::ports::{AccessMapping, MappingPreparation, QueryFragment, RelationMapping};
use crate::{Error, Result};

type MappingCache = Vec<(Arc<dyn AccessMapping>, Arc<dyn AccessMapping>)>;

pub(crate) struct FrozenMapping {
    inner: Arc<dyn AccessMapping>,
    model: QueryModel,
    stages: Vec<Document>,
    stage_bytes: Vec<Vec<u8>>,
    originals: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
    relations: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
}

impl FrozenMapping {
    pub(crate) fn freeze(
        inner: Arc<dyn AccessMapping>,
        cache: &mut MappingCache,
        depth: usize,
    ) -> Result<Arc<dyn AccessMapping>> {
        if depth > 16 || cache.len() > 1000 {
            return Err(Error::ValidationError("Mapping 来源过深或过多".into()));
        }
        if let Some((_, frozen)) = cache.iter().find(|(original, _)| Arc::ptr_eq(original, &inner)) {
            return Ok(frozen.clone());
        }

        let model = inner.model().clone();
        model.validate()?;
        let stages = inner.stages();
        validate_stages(&stages)?;
        let stage_bytes = encode_stages(&stages)?;

        let originals = inner.relations();
        let mut relations = BTreeMap::new();
        for (key, mapping) in &originals {
            if mapping.model().output != QueryOutput::Object {
                return Err(Error::ValidationError("来源 Mapping 必须提供完整对象".into()));
            }
            relations.insert(key.clone(), Self::freeze(mapping.clone(), cache, depth + 1)?);
        }

        let frozen: Arc<dyn AccessMapping> =
            Arc::new(Self { inner: inner.clone(), model, stages, stage_bytes, originals, relations });
        cache.push((inner, frozen.clone()));
        Ok(frozen)
    }

    fn ensure_model(&self) -> Result<()> {
        if self.inner.model() != &self.model || encode_stages(&self.inner.stages())? != self.stage_bytes {
            return Err(Error::ValidationError("Mapping 描述在构造后改变".into()));
        }
        Ok(())
    }
}

impl AccessMapping for FrozenMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }

    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.relations.clone()
    }

    fn stages(&self) -> Vec<Document> {
        self.stages.clone()
    }

    fn valid_until(&self, context: &AccessContext) -> Option<Instant> {
        self.inner.valid_until(context)
    }

    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        self.inner.preparation()
    }

    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson> {
        self.ensure_model()?;
        self.inner.key(context, prepared)
    }

    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment> {
        self.ensure_model()?;
        self.inner.records(context, prepared)
    }

    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        self.ensure_model()?;
        self.inner.predicate(fact, context, prepared)
    }

    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        self.ensure_model()?;
        let actual = self.inner.relation(relation, context, prepared)?;
        let original = self
            .originals
            .get(relation)
            .ok_or_else(|| Error::ValidationError("Mapping 来源未在启动描述中登记".into()))?;
        if !Arc::ptr_eq(original, &actual.mapping) {
            return Err(Error::ValidationError("Mapping 来源在构造后改变".into()));
        }
        Ok(RelationMapping { expected_ids: actual.expected_ids, mapping: self.relations[relation].clone() })
    }
}

fn encode_stages(stages: &[Document]) -> Result<Vec<Vec<u8>>> {
    stages
        .iter()
        .map(|stage| stage.to_vec().map_err(|_| Error::ValidationError("Mapping 固定阶段无法编码".into())))
        .collect()
}

/// 固定准备只物化完整关系，不能提前过滤、展开或分页根记录。
fn validate_stages(stages: &[Document]) -> Result<()> {
    for stage in stages {
        if stage.len() != 1
            || !stage.keys().all(|key| matches!(key.as_str(), "$lookup" | "$set" | "$addFields"))
        {
            return Err(Error::ValidationError("Mapping 固定准备不得改变根记录集合".into()));
        }
        reject_writes(&Bson::Document(stage.clone()))?;
    }
    Ok(())
}

fn reject_writes(value: &Bson) -> Result<()> {
    match value {
        Bson::Document(document) => {
            for (key, value) in document {
                if matches!(key.as_str(), "$out" | "$merge") {
                    return Err(Error::ValidationError("Mapping 固定准备必须只读".into()));
                }
                reject_writes(value)?;
            }
        },
        Bson::Array(values) => {
            for value in values {
                reject_writes(value)?;
            }
        },
        _ => {},
    }
    Ok(())
}

pub(crate) fn validate_rule(rule: &AccessRule, mapping: &dyn AccessMapping, depth: usize) -> Result<()> {
    if depth > 16 {
        return Err(Error::ValidationError("授权来源嵌套超限".into()));
    }
    match rule {
        AccessRule::Fact(fact) if !mapping.model().facts.contains(&fact.kind()) => {
            return Err(Error::ValidationError(format!(
                "Mapping {}（{}）缺少固定事实 {:?}",
                mapping.model().key.as_str(),
                mapping.model().resource,
                fact.kind()
            )));
        },
        AccessRule::Required { facts, rule } => {
            if !facts.is_subset(&mapping.model().facts) {
                return Err(Error::ValidationError(format!(
                    "Mapping {}（{}）缺少必需事实 {:?}",
                    mapping.model().key.as_str(),
                    mapping.model().resource,
                    facts.difference(&mapping.model().facts).collect::<Vec<_>>()
                )));
            }
            validate_rule(rule, mapping, depth + 1)?;
        },
        AccessRule::All(rules) | AccessRule::Any(rules) => {
            for rule in rules {
                validate_rule(rule, mapping, depth + 1)?;
            }
        },
        AccessRule::Independent { rule, .. } => validate_rule(rule, mapping, depth + 1)?,
        AccessRule::RelationPresence { relation, .. } => {
            require_relation(mapping, relation)?;
        },
        AccessRule::Related { relation, resource, mode, rule } => {
            let child = require_relation(mapping, relation)?;
            if child.model().resource != *resource || child.model().output != QueryOutput::Object {
                return Err(Error::ValidationError("来源 Mapping 资源或输出错配".into()));
            }
            if *mode == RelationMatch::Any && mapping.model().output == QueryOutput::Object {
                return Err(Error::ValidationError("任一来源不能输出完整对象".into()));
            }
            validate_rule(rule, child.as_ref(), depth + 1)?;
        },
        AccessRule::RecordReadGrant(resource) if mapping.model().resource != resource.code() => {
            return Err(Error::ValidationError("记录授权与 Mapping 资源错配".into()));
        },
        _ => {},
    }
    Ok(())
}

fn require_relation(mapping: &dyn AccessMapping, key: &RelationKey) -> Result<Arc<dyn AccessMapping>> {
    mapping
        .relations()
        .remove(key)
        .ok_or_else(|| Error::ValidationError(format!("缺少固定来源 Mapping：{}", key.as_str())))
}
