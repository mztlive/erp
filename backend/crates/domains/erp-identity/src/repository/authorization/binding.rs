//! 启动描述与实际 Mapping 的连接；运行时不得替换模型或来源实现。
use std::collections::BTreeMap;
use std::sync::Arc;

use mongodb::bson::Bson;

use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, MappingFacts, QueryModel, QueryOutput, RelationKey, RelationMatch,
};
use crate::ports::{AccessMapping, MappingPreparation, QueryFragment, RelationMapping};
use crate::{Error, Result};

type MappingCache = Vec<(Arc<dyn AccessMapping>, Arc<dyn AccessMapping>)>;

pub(crate) struct FrozenMapping {
    inner: Arc<dyn AccessMapping>,
    model: QueryModel,
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
        let originals = inner.relations();
        let mut relations = BTreeMap::new();
        for (key, mapping) in &originals {
            if mapping.model().output != QueryOutput::Object {
                return Err(Error::ValidationError("来源 Mapping 必须提供完整对象".into()));
            }
            relations.insert(key.clone(), Self::freeze(mapping.clone(), cache, depth + 1)?);
        }
        let frozen: Arc<dyn AccessMapping> =
            Arc::new(Self { inner: inner.clone(), model, originals, relations });
        cache.push((inner, frozen.clone()));
        Ok(frozen)
    }

    fn ensure_model(&self) -> Result<()> {
        if self.inner.model() != &self.model {
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

pub(crate) fn validate_rule(rule: &AccessRule, mapping: &dyn AccessMapping, depth: usize) -> Result<()> {
    if depth > 16 {
        return Err(Error::ValidationError("授权来源嵌套超限".into()));
    }
    match rule {
        AccessRule::Fact(fact) if !mapping.model().facts.contains(&fact.kind()) => {
            return Err(Error::ValidationError("Mapping 缺少固定规则所需事实".into()));
        },
        AccessRule::Required { facts, rule } => {
            if !facts.is_subset(&mapping.model().facts) {
                return Err(Error::ValidationError("Mapping 缺少必需事实维度".into()));
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
