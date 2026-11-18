//! 责任预览的入口资格、整批解析和逐行诊断共用同一快照。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::AccessTarget;
use erp_permissions::{Permission, admin};
use erp_procurement::dto::procurement_responsibility::{
    ProcurementResponsibilityResolutionView, ProcurementResponsibilityResolveLineView,
    ProcurementResponsibilityResolveRequest, ProcurementResponsibilityResolveView,
};
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityResolutionBatch, ResolutionPurpose,
};
use persistence_core::{Executor, Transactional};

use super::{ProcurementResponsibilityProcess, ResolutionInput};
use crate::{Error, Result};

impl ProcurementResponsibilityProcess {
    /// 在同一快照中核验入口资格并逐行预览采购责任。
    ///
    /// # 参数
    /// * `request` - 预览场景和稳定需求行。
    /// * `actor` - 当前已认证账号。
    ///
    /// # 返回
    /// 返回与请求顺序一致的成功结果或逐行诊断。
    ///
    /// # 错误
    /// 缺少预览权限、输入非法或事务失败时拒绝整个请求。
    pub async fn resolve_preview(
        &self,
        request: ProcurementResponsibilityResolveRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityResolveView> {
        let source = Self::new(self.db.clone(), self.rbac.clone());
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { preview(&source, &actor, request, executor).await })
            })
            .await
    }
}

/// 预览只消费入口授权及按原执行器解析整批需求的能力。
#[async_trait]
trait PreviewSource: Sync {
    async fn authorize(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn batch(
        &self,
        inputs: &[ResolutionInput],
        purpose: ResolutionPurpose,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementResponsibilityResolutionView>>;
}

#[async_trait]
impl PreviewSource for ProcurementResponsibilityProcess {
    async fn authorize(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.rbac.require(actor, permission, AccessTarget::Operation, executor).await?;
        Ok(())
    }

    async fn batch(
        &self,
        inputs: &[ResolutionInput],
        purpose: ResolutionPurpose,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementResponsibilityResolutionView>> {
        self.preview_batch(inputs, purpose, executor).await
    }
}

/// 正常整批与逐行诊断共用原快照；嵌套统一资格检查允许推进读取阶段。
async fn preview(
    source: &impl PreviewSource,
    actor: &AuditActor,
    request: ProcurementResponsibilityResolveRequest,
    executor: &mut dyn Executor,
) -> Result<ProcurementResponsibilityResolveView> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("采购责任预览必须使用同一快照事务".into()));
    }
    source.authorize(actor, &admin::PROCUREMENT_RESPONSIBILITY_RESOLVE, executor).await?;

    let view = preview_lines(source, request, executor).await?;
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("采购责任预览已离开原快照事务".into()));
    }
    Ok(view)
}

/// 整批失败时保留逐行诊断，所有分支只使用入口传入的执行器。
async fn preview_lines(
    source: &impl PreviewSource,
    request: ProcurementResponsibilityResolveRequest,
    executor: &mut dyn Executor,
) -> Result<ProcurementResponsibilityResolveView> {
    let inputs = request
        .lines
        .into_iter()
        .map(|line| ResolutionInput::new(line.line_key, line.sku_id))
        .collect::<erp_core::Result<Vec<_>>>()
        .map_err(Error::Logic)?;
    ProcurementResponsibilityResolutionBatch::new(&inputs).map_err(Error::Logic)?;

    if let Ok(views) = source.batch(&inputs, request.purpose, executor).await {
        return Ok(ProcurementResponsibilityResolveView {
            lines: views.into_iter().map(success_preview).collect(),
        });
    }

    let mut lines = Vec::with_capacity(inputs.len());
    for input in inputs {
        let line_key = input.line_key.clone();
        let result = source.batch(std::slice::from_ref(&input), request.purpose, executor).await;
        lines.push(match result {
            Ok(views) => {
                let resolution = views.into_iter().next().expect("单行解析必须返回单行");
                success_preview(resolution)
            },
            Err(error) => failed_preview(line_key, error),
        });
    }
    Ok(ProcurementResponsibilityResolveView { lines })
}

/// 将成功解析转换为逐行预览视图。
fn success_preview(
    resolution: ProcurementResponsibilityResolutionView,
) -> ProcurementResponsibilityResolveLineView {
    ProcurementResponsibilityResolveLineView {
        line_key: resolution.line_key,
        resolved: true,
        owner_user_id: Some(resolution.owner_user_id),
        owner_name: Some(resolution.owner_name),
        rule_id: Some(resolution.rule_id),
        rule_type: Some(resolution.rule_type),
        error: None,
    }
}

/// 将单行解析错误转换为失败预览。
fn failed_preview(line_key: String, error: Error) -> ProcurementResponsibilityResolveLineView {
    ProcurementResponsibilityResolveLineView {
        line_key,
        resolved: false,
        owner_user_id: None,
        owner_name: None,
        rule_id: None,
        rule_type: None,
        error: Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_core::ids::SkuId;
    use erp_identity::testing::unavailable_authorization;
    use erp_procurement::dto::procurement_responsibility::ProcurementResponsibilityResolveLineRequest;
    use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRuleType;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    struct Fixture {
        denied: bool,
        failed_line: Option<&'static str>,
        revoke_snapshot: bool,
        calls: Mutex<Vec<(String, usize)>>,
    }

    impl Fixture {
        fn new() -> Self {
            Self { denied: false, failed_line: None, revoke_snapshot: false, calls: Mutex::default() }
        }

        fn record(&self, event: String, executor: &mut dyn Executor) {
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            self.calls.lock().unwrap().push((event, executor as *mut dyn Executor as *mut () as usize));
        }
    }

    #[async_trait]
    impl PreviewSource for Fixture {
        async fn authorize(
            &self,
            actor: &AuditActor,
            permission: &Permission,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!(actor.id(), "actor");
            self.record(permission.to_string(), executor);
            if self.denied {
                return Err(Error::Forbidden("缺少采购责任预览资格".into()));
            }
            Ok(())
        }

        async fn batch(
            &self,
            inputs: &[ResolutionInput],
            purpose: ResolutionPurpose,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ProcurementResponsibilityResolutionView>> {
            assert_eq!(purpose, ResolutionPurpose::Inquiry);
            let keys = inputs.iter().map(|input| input.line_key.as_str()).collect::<Vec<_>>();
            self.record(format!("batch:{}", keys.join(",")), executor);
            executor.next_read();
            if self.revoke_snapshot {
                let _ = executor.session();
            }
            if self.failed_line.is_some_and(|failed| keys.contains(&failed)) {
                return Err(Error::BusinessLogicError("商品未配置责任规则".into()));
            }
            Ok(inputs
                .iter()
                .map(|input| ProcurementResponsibilityResolutionView {
                    line_key: input.line_key.clone(),
                    owner_user_id: "buyer".into(),
                    owner_name: "采购员".into(),
                    rule_id: "rule".into(),
                    rule_type: ProcurementResponsibilityRuleType::Inquiry,
                })
                .collect())
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
    }

    fn request() -> ProcurementResponsibilityResolveRequest {
        ProcurementResponsibilityResolveRequest {
            purpose: ResolutionPurpose::Inquiry,
            lines: ["first", "second"]
                .into_iter()
                .map(|line_key| ProcurementResponsibilityResolveLineRequest {
                    line_key: line_key.into(),
                    sku_id: SkuId::new(format!("sku-{line_key}")),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn preview_authorizes_before_one_batch_and_preserves_the_original_snapshot() {
        let source = Fixture::new();
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;

        let view = preview(&source, &actor(), request(), &mut executor).await.unwrap();

        assert_eq!(
            view.lines.iter().map(|line| line.line_key.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(
            view.lines.iter().all(|line| line.resolved && line.owner_user_id.as_deref() == Some("buyer"))
        );

        assert_eq!(
            *source.calls.lock().unwrap(),
            [
                (admin::PROCUREMENT_RESPONSIBILITY_RESOLVE.to_string(), expected),
                ("batch:first,second".into(), expected)
            ]
        );
    }

    #[tokio::test]
    async fn denied_preview_does_not_read_any_rules_or_owner_facts() {
        let mut source = Fixture::new();
        source.denied = true;

        let error =
            preview(&source, &actor(), request(), &mut SnapshotExecutor::default()).await.unwrap_err();

        assert!(matches!(error, Error::Forbidden(message) if message == "缺少采购责任预览资格"));
        let calls = source.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, admin::PROCUREMENT_RESPONSIBILITY_RESOLVE.to_string());
    }

    #[tokio::test]
    async fn failed_batch_keeps_each_line_diagnostic_in_the_original_snapshot() {
        let mut source = Fixture::new();
        source.failed_line = Some("second");
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;

        let view = preview(&source, &actor(), request(), &mut executor).await.unwrap();

        assert!(view.lines[0].resolved);
        assert!(!view.lines[1].resolved);
        assert_eq!(view.lines[1].error.as_deref(), Some("业务逻辑错误: 商品未配置责任规则"));

        assert_eq!(
            *source.calls.lock().unwrap(),
            [
                (admin::PROCUREMENT_RESPONSIBILITY_RESOLVE.to_string(), expected),
                ("batch:first,second".into(), expected),
                ("batch:first".into(), expected),
                ("batch:second".into(), expected),
            ]
        );
    }

    #[tokio::test]
    async fn preview_rejects_snapshot_revocation_during_owner_qualification() {
        let mut source = Fixture::new();
        source.revoke_snapshot = true;
        let error =
            preview(&source, &actor(), request(), &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "采购责任预览已离开原快照事务")
        );
    }

    #[tokio::test]
    async fn preview_rejects_non_snapshot_execution_and_unavailable_real_authorization() {
        let fixture = Fixture::new();

        let error = preview(&fixture, &actor(), request(), &mut ReadExecutor::default()).await.unwrap_err();

        assert!(
            matches!(error, Error::ValidationError(message) if message == "采购责任预览必须使用同一快照事务")
        );
        assert!(fixture.calls.lock().unwrap().is_empty());

        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("procurement_preview");
        let source = ProcurementResponsibilityProcess::new(db, unavailable_authorization());

        let error =
            preview(&source, &actor(), request(), &mut SnapshotExecutor::default()).await.unwrap_err();

        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
