//! 测试中的不可用授权设施，用于证明输入拒绝先于任何身份或业务读取。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use super::AuthorizationService;
use super::facilities::Facilities;
use crate::entity::authorization::{AccessContext, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

impl AuthorizationService {
    pub(crate) fn unavailable_for_test() -> Arc<Self> {
        Arc::new(Self {
            facilities: Arc::new(Unavailable),
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
            token: Arc::new(()),
        })
    }
}
struct Unavailable;

fn unavailable() -> Error {
    Error::Internal("测试授权设施不可用".into())
}

#[async_trait]
impl Facilities for Unavailable {
    async fn permissions(
        &self,
        _: &AuditActor,
        _: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        Err(unavailable())
    }
    async fn scope(
        &self,
        _: &AccessContext,
        _: &Permission,
        _: &[Permission],
        _: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        Err(unavailable())
    }
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Err(unavailable())
    }
    async fn grants(
        &self,
        _: &AccessContext,
        _: ResourceKind,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Err(unavailable())
    }
}
