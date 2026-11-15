//! 命令回放先核验当前资格和真实供给，所有读取使用原快照。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_permissions::{Permission, internal, supplier_offering};

use super::*;

/// 普通命令继续使用已授权对象；新建命令没有回执时继续构造候选。
pub(super) enum Replay<T, O> {
    Stored(T),
    Absent(Option<O>),
}

/// 回包中的真实供给身份用于创建回放的对象授权。
trait OfferingResult: DeserializeOwned {
    fn offering_id(&self) -> &str;
}

impl OfferingResult for CreateSupplierOfferingResult {
    fn offering_id(&self) -> &str {
        &self.offering_id
    }
}
impl OfferingResult for ReviseSupplierOfferingResult {
    fn offering_id(&self) -> &str {
        &self.offering_id
    }
}
impl OfferingResult for UpdateSupplierOfferingAvailabilityResult {
    fn offering_id(&self) -> &str {
        &self.offering_id
    }
}

#[async_trait]
trait CommandResultPort: Sync {
    type Object: Send;
    async fn qualify_create(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()>;
    async fn authorize(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Self::Object>;
    async fn command(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingCommand>>;
}

#[async_trait]
impl CommandResultPort for SupplierOfferingService {
    type Object = SupplierOffering;
    async fn qualify_create(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.access().ensure_writable(actor, &supplier_offering::CREATE, executor).await
    }
    async fn authorize(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOffering> {
        self.access().require_offering(actor, permission, id, executor).await
    }
    async fn command(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingCommand>> {
        self.command_record(key, executor).await
    }
}

/// 原命令的路径目标、幂等键、操作及载荷指纹；不包含授权结果。
struct CommandIdentity<'a> {
    target: Option<&'a str>,
    key: &'a str,
    operation: &'a str,
    fingerprint: &'a str,
}

/// 已知路径目标先授权；创建回执先查资格，再按原结果定位真实供给授权。
async fn replay<T: OfferingResult, P: CommandResultPort>(
    port: &P,
    actor: &AuditActor,
    permission: &Permission,
    identity: CommandIdentity<'_>,
    executor: &mut dyn Executor,
) -> Result<Replay<T, P::Object>> {
    let object = if let Some(id) = identity.target {
        Some(port.authorize(actor, permission, id, executor).await?)
    } else {
        port.qualify_create(actor, executor).await?;
        None
    };
    let Some(command) = port.command(identity.key, executor).await? else {
        return Ok(Replay::Absent(object));
    };
    command
        .ensure_replayable(identity.operation, identity.fingerprint)
        .map_err(|e| Error::ConflictError(e.to_string()))?;
    let result: T = command.replay_result().map_err(|e| Error::Internal(e.to_string()))?;
    if let Some(id) = identity.target {
        if result.offering_id() != id {
            return Err(Error::Internal("供给命令回执与目标不匹配".into()));
        }
    } else {
        port.authorize(actor, permission, result.offering_id(), executor).await?;
    }
    Ok(Replay::Stored(result))
}

/// 创建预检及提交结果恢复共用当前资格和原供给授权。
pub(super) async fn created(
    service: &SupplierOfferingService,
    request: &CreateSupplierOfferingRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Replay<CreateSupplierOfferingResult, SupplierOffering>> {
    replay(
        service,
        actor,
        &internal::SUPPLIER_OFFERING_CREATED_RESULT_READ,
        CommandIdentity {
            target: None,
            key: &request.idempotency_key,
            operation: CREATE_OFFERING_COMMAND,
            fingerprint: &request.command_fingerprint()?,
        },
        executor,
    )
    .await
}

/// 修订预检及结果恢复均在读取回执前核验真实对象。
pub(super) async fn revised(
    service: &SupplierOfferingService,
    id: &str,
    request: &ReviseSupplierOfferingRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Replay<ReviseSupplierOfferingResult, SupplierOffering>> {
    replay(
        service,
        actor,
        &supplier_offering::REVISE,
        CommandIdentity {
            target: Some(id),
            key: &request.idempotency_key,
            operation: REVISE_OFFERING_COMMAND,
            fingerprint: &request.command_fingerprint(id)?,
        },
        executor,
    )
    .await
}

/// 可供更新使用身份域固定的完整入口，不在业务层组合权限。
pub(super) async fn availability(
    service: &SupplierOfferingService,
    id: &str,
    request: &UpdateSupplierOfferingAvailabilityRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Replay<UpdateSupplierOfferingAvailabilityResult, SupplierOffering>> {
    replay(
        service,
        actor,
        &internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE,
        CommandIdentity {
            target: Some(id),
            key: &request.idempotency_key,
            operation: UPDATE_OFFERING_AVAILABILITY_COMMAND,
            fingerprint: &request.command_fingerprint(id)?,
        },
        executor,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use persistence_core::ReadExecutor;

    use super::*;

    const FINGERPRINT: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    struct Recorder {
        pointer: usize,
        allowed_actor: &'static str,
        qualify: bool,
        authorize: bool,
        receipt: Option<SupplierOfferingCommand>,
        calls: Mutex<Vec<String>>,
    }

    impl Recorder {
        /// 所有资格、回执和对象读取均使用调用方原执行器。
        fn record(&self, name: String, executor: &mut dyn Executor) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            self.calls.lock().unwrap().push(name);
        }
    }

    #[async_trait]
    impl CommandResultPort for Recorder {
        type Object = String;
        async fn qualify_create(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
            self.record(format!("qualify:{}", actor.id()), executor);
            if !self.qualify || actor.id() != self.allowed_actor {
                return Err(Error::Forbidden("create qualification revoked".into()));
            }
            Ok(())
        }
        async fn authorize(
            &self,
            actor: &AuditActor,
            permission: &Permission,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<String> {
            self.record(format!("authorize:{}:{permission}:{id}", actor.id()), executor);
            if !self.authorize || actor.id() != self.allowed_actor {
                return Err(Error::Forbidden("current scope revoked".into()));
            }
            Ok(id.to_string())
        }
        async fn command(
            &self,
            key: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierOfferingCommand>> {
            assert_eq!(key, "key");
            self.record("command".into(), executor);
            Ok(self.receipt.clone())
        }
    }

    /// 测试回执保持生产回包中的供给身份和稳定字段。
    fn result() -> CreateSupplierOfferingResult {
        CreateSupplierOfferingResult {
            offering_id: "offering".into(),
            revision_id: "revision".into(),
            availability_id: "availability".into(),
            revision_no: 1,
            status: OfferingStatus::Active,
        }
    }

    /// 同键同载荷的回执不包含任何当前资格证明。
    fn recorder(executor: &mut ReadExecutor) -> Recorder {
        Recorder {
            pointer: executor as *mut ReadExecutor as usize,
            allowed_actor: "actor",
            qualify: true,
            authorize: true,
            receipt: Some(
                SupplierOfferingCommand::with_result(
                    "command",
                    "key",
                    CREATE_OFFERING_COMMAND,
                    FINGERPRINT,
                    &result(),
                )
                .unwrap(),
            ),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 生产回放编排验证资格和对象授权，未执行写入。
    async fn invoke(
        recorder: &Recorder,
        actor: &str,
        target: Option<&str>,
        executor: &mut ReadExecutor,
    ) -> Result<Replay<CreateSupplierOfferingResult, String>> {
        let actor = AuditActor::new(actor.into(), actor.into(), AccountKind::Admin);
        replay(
            recorder,
            &actor,
            &internal::SUPPLIER_OFFERING_CREATED_RESULT_READ,
            CommandIdentity {
                target,
                key: "key",
                operation: CREATE_OFFERING_COMMAND,
                fingerprint: FINGERPRINT,
            },
            executor,
        )
        .await
    }

    /// 当前创建资格撤销时，不读取命令回执或实际业务对象。
    #[tokio::test]
    async fn create_replay_rejects_revoked_qualification_before_receipt_read() {
        let mut executor = ReadExecutor::default();
        let mut recorder = recorder(&mut executor);
        recorder.qualify = false;
        assert!(matches!(invoke(&recorder, "actor", None, &mut executor).await,
            Err(Error::Forbidden(reason)) if reason == "create qualification revoked"));
        assert_eq!(*recorder.calls.lock().unwrap(), ["qualify:actor"]);
    }

    /// 创建回放定位原供给后核验当前对象范围，不能仅凭原载荷回包。
    #[tokio::test]
    async fn create_replay_requires_the_persisted_offering_current_scope() {
        let mut executor = ReadExecutor::default();
        let mut recorder = recorder(&mut executor);
        recorder.authorize = false;
        assert!(matches!(invoke(&recorder, "actor", None, &mut executor).await,
            Err(Error::Forbidden(reason)) if reason == "current scope revoked"));
        assert_eq!(
            *recorder.calls.lock().unwrap(),
            ["qualify:actor", "command", "authorize:actor:supplier_offering:created_result_read:offering",]
        );
    }

    /// 已知目标在读取回执前授权；未获授权的其他账号不能借用相同命令资料。
    #[tokio::test]
    async fn existing_target_replay_stops_before_receipt_for_unauthorized_actor() {
        let mut executor = ReadExecutor::default();
        let recorder = recorder(&mut executor);
        assert!(matches!(invoke(&recorder, "other", Some("offering"), &mut executor).await,
            Err(Error::Forbidden(reason)) if reason == "current scope revoked"));
        assert_eq!(
            *recorder.calls.lock().unwrap(),
            ["authorize:other:supplier_offering:created_result_read:offering"]
        );
    }

    /// 已授权原结果可返回，回执目标必须与实际路径目标一致。
    #[tokio::test]
    async fn current_authorized_replay_keeps_the_result_and_rejects_mismatched_target() {
        let mut executor = ReadExecutor::default();
        let recorder = recorder(&mut executor);
        let Replay::Stored(actual) =
            invoke(&recorder, "actor", Some("offering"), &mut executor).await.unwrap()
        else {
            panic!("matching receipt must replay")
        };
        assert_eq!(actual.offering_id, "offering");
        assert_eq!(actual.revision_id, "revision");
        assert_eq!(actual.availability_id, "availability");
        assert_eq!(actual.revision_no, 1);
        assert_eq!(actual.status, OfferingStatus::Active);
        assert!(matches!(invoke(&recorder, "actor", Some("other-offering"), &mut executor).await,
            Err(Error::Internal(reason)) if reason == "供给命令回执与目标不匹配"));
    }

    /// 没有回执时复用已授权对象，不再次解析相同快照中的资格和事实。
    #[tokio::test]
    async fn missing_receipt_preserves_the_authorized_object_for_normal_write_preparation() {
        let mut executor = ReadExecutor::default();
        let mut recorder = recorder(&mut executor);
        recorder.receipt = None;
        let Replay::Absent(Some(offering)) =
            invoke(&recorder, "actor", Some("offering"), &mut executor).await.unwrap()
        else {
            panic!("new command must retain its authorized offering")
        };
        assert_eq!(offering, "offering");
        assert_eq!(
            *recorder.calls.lock().unwrap(),
            ["authorize:actor:supplier_offering:created_result_read:offering", "command"]
        );
    }
}
