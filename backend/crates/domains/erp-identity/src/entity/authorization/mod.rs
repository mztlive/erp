//! 业务字段无关的授权语义；业务映射只解释事实，不组合授权规则。

mod mapping;
mod object;
mod rule;
mod target;

pub use mapping::{QueryModel, QueryOutput};
pub use rule::AccessFact;
pub(crate) use rule::AccessRule;
pub use target::{
    AccessContext, AccessTarget, ObjectFacts, QueryModelKey, RelatedObjects, RelationKey, RelationMatch,
};

mod targets;
pub use targets::ResourceTargets;

mod facts;
pub use facts::{FactKind, FactValue};
