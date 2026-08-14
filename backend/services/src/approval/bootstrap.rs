//! 启动期确定性审批定义 bootstrap。

use database::{ApprovalExt, NoTransaction, Transactional};
use entities::{
    approval::{
        ApprovalDefinition, ApprovalDefinitionData, ApprovalDefinitionStatus, ApprovalRuntimeKind,
        ApprovalStepDefinition, ApprovalStepDefinitionData,
    },
    common::time::Instant,
    ApprovalDefinitionId, ApprovalStepDefinitionId,
};
use mongodb::Database;

use crate::errors::{Error, Result};

use super::registry::{registered_definitions, step_is_registered, RegisteredApprovalDefinition};

const BOOTSTRAP_ACTOR: &str = "system:approval-bootstrap";

/// 确保部署清单声明的审批定义与步骤已原子发布。
///
/// 已存在的同版本定义必须与编译期清单逐字段一致；任何漂移都会阻止启动，
/// 不会静默改写运行语义。缺失定义按稳定 ID 在单个 MongoDB 事务内创建步骤并发布。
///
/// # 错误
/// 定义漂移、注册表不完整、并发冲突或数据库事务失败时返回错误。
pub async fn ensure_approval_definitions(db: &Database) -> Result<()> {
    for registered in registered_definitions() {
        ensure_one(db, registered).await?;
    }
    Ok(())
}

async fn ensure_one(db: &Database, registered: &RegisteredApprovalDefinition) -> Result<()> {
    validate_registered_definition(registered)?;
    if let Some(existing) = db
        .approval_definitions()
        .find_by_key_version(registered.definition_key, registered.version, &mut NoTransaction)
        .await?
    {
        return verify_existing(db, existing, registered).await;
    }

    let (definition, steps) = build_definition(registered)?;
    let db = db.clone();
    let transaction_db = db.clone();
    let client = db.client().clone();
    let create_result = client
        .with_transaction(move |session| {
            Box::pin(async move {
                if transaction_db
                    .approval_definitions()
                    .find_by_key_version(&definition.definition_key, definition.definition_version, session)
                    .await?
                    .is_some()
                {
                    return Ok::<(), Error>(());
                }
                let mut previously_published = transaction_db
                    .approval_definitions()
                    .find_published_by_key(&definition.definition_key, session)
                    .await?;
                if let Some(current) = previously_published.as_ref() {
                    ensure_forward_definition_upgrade(
                        &definition.definition_key,
                        current.definition_version,
                        definition.definition_version,
                    )?;
                }
                transaction_db
                    .approval()
                    .create_draft_with_steps(&definition, &steps, session)
                    .await?;
                if let Some(current) = previously_published.as_mut() {
                    current.retire()?;
                    transaction_db
                        .approval_definitions()
                        .update(current, session)
                        .await?;
                }
                let mut published = definition;
                published.publish(BOOTSTRAP_ACTOR, Instant::now())?;
                transaction_db
                    .approval_definitions()
                    .update(&mut published, session)
                    .await?;
                Ok(())
            })
        })
        .await;
    if let Err(error) = create_result {
        let concurrently_created = db
            .approval_definitions()
            .find_by_key_version(registered.definition_key, registered.version, &mut NoTransaction)
            .await?
            .is_some();
        if !concurrently_created {
            return Err(error);
        }
    }

    let existing = db
        .approval_definitions()
        .find_by_key_version(registered.definition_key, registered.version, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::Internal("审批定义 bootstrap 提交后无法读取".to_string()))?;
    verify_existing(&db, existing, registered).await
}

/// 要求 bootstrap 只能把已发布定义单向升级到更高业务版本。
fn ensure_forward_definition_upgrade(
    definition_key: &str,
    published_version: u32,
    target_version: u32,
) -> Result<()> {
    if target_version <= published_version {
        return Err(Error::Internal(format!(
            "审批定义 {definition_key} 已发布 v{published_version}，禁止回退或重复发布 v{target_version}"
        )));
    }
    Ok(())
}

fn build_definition(
    registered: &RegisteredApprovalDefinition,
) -> Result<(ApprovalDefinition, Vec<ApprovalStepDefinition>)> {
    let definition_id = definition_id(registered.definition_key, registered.version);
    let definition = ApprovalDefinition::new(
        definition_id.clone(),
        ApprovalDefinitionData {
            definition_key: registered.definition_key.to_string(),
            definition_version: registered.version,
            name: registered.name.to_string(),
            runtime_kind: ApprovalRuntimeKind::Internal,
            external_definition_id: None,
        },
    )?;
    let steps = registered
        .steps
        .iter()
        .map(|step| {
            ApprovalStepDefinition::new(
                step_definition_id(registered.definition_key, registered.version, step.sequence_no),
                ApprovalStepDefinitionData {
                    approval_definition_id: definition_id.clone(),
                    step_key: step.step_key.to_string(),
                    sequence_no: step.sequence_no,
                    work_item_type: step.work_item_type,
                    handler_key: step.handler_key.to_string(),
                    assignment_mode: step.assignment_mode,
                    assignee_resolver_key: step.resolver_key.to_string(),
                    allowed_decisions: step.allowed_decisions.to_vec(),
                },
            )
            .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((definition, steps))
}

async fn verify_existing(
    db: &Database,
    definition: ApprovalDefinition,
    registered: &RegisteredApprovalDefinition,
) -> Result<()> {
    let expected_definition_id = definition_id(registered.definition_key, registered.version);
    if definition.base.id != expected_definition_id.to_string()
        || definition.status != ApprovalDefinitionStatus::Published
        || definition.name != registered.name
        || definition.runtime_kind != ApprovalRuntimeKind::Internal
        || definition.external_definition_id.is_some()
    {
        return Err(definition_drift(registered));
    }
    let steps = db
        .approval_step_definitions()
        .list_by_definition(
            &ApprovalDefinitionId::new(&definition.base.id),
            &mut NoTransaction,
        )
        .await?;
    if steps.len() != registered.steps.len() {
        return Err(definition_drift(registered));
    }
    for (actual, expected) in steps.iter().zip(registered.steps) {
        let expected_step_id = step_definition_id(
            registered.definition_key,
            registered.version,
            expected.sequence_no,
        );
        if actual.base.id != expected_step_id.to_string()
            || actual.approval_definition_id != expected_definition_id
            || actual.step_key != expected.step_key
            || actual.sequence_no != expected.sequence_no
            || actual.work_item_type != expected.work_item_type
            || actual.handler_key != expected.handler_key
            || actual.assignment_mode != expected.assignment_mode
            || actual.assignee_resolver_key != expected.resolver_key
            || actual.allowed_decisions != expected.allowed_decisions
        {
            return Err(definition_drift(registered));
        }
    }
    Ok(())
}

fn validate_registered_definition(registered: &RegisteredApprovalDefinition) -> Result<()> {
    if registered.steps.is_empty() {
        return Err(Error::Internal("审批定义至少需要一个步骤".to_string()));
    }
    for (index, step) in registered.steps.iter().enumerate() {
        if step.sequence_no != u32::try_from(index + 1).unwrap_or(u32::MAX) || !step_is_registered(step) {
            return Err(Error::Internal(format!(
                "审批定义 {} v{} 的步骤注册不完整",
                registered.definition_key, registered.version
            )));
        }
    }
    Ok(())
}

fn definition_id(definition_key: &str, version: u32) -> ApprovalDefinitionId {
    ApprovalDefinitionId::new(format!("approval-definition-{definition_key}-v{version}"))
}

fn step_definition_id(definition_key: &str, version: u32, sequence_no: u32) -> ApprovalStepDefinitionId {
    ApprovalStepDefinitionId::new(format!(
        "approval-step-definition-{definition_key}-v{version}-{sequence_no}"
    ))
}

fn definition_drift(registered: &RegisteredApprovalDefinition) -> Error {
    Error::Internal(format!(
        "审批定义 {} v{} 与当前编译期清单不一致；请发布新版本，禁止改写已发布定义",
        registered.definition_key, registered.version
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        definition_id, ensure_forward_definition_upgrade, step_definition_id, validate_registered_definition,
    };
    use crate::approval::registry::definition;

    #[test]
    fn bootstrap_ids_are_stable_across_restarts() {
        assert_eq!(
            definition_id("CARD_SALES_APPROVAL", 1).to_string(),
            "approval-definition-CARD_SALES_APPROVAL-v1"
        );
        assert_eq!(
            step_definition_id("CARD_SALES_APPROVAL", 1, 2).to_string(),
            "approval-step-definition-CARD_SALES_APPROVAL-v1-2"
        );
    }

    #[test]
    fn registered_definition_passes_publish_validation() {
        let registered = definition("CARD_SALES_APPROVAL").unwrap();
        assert!(validate_registered_definition(registered).is_ok());
    }

    #[test]
    fn bootstrap_only_retires_published_definition_for_a_forward_upgrade() {
        assert!(ensure_forward_definition_upgrade("CARD_SALES_APPROVAL", 1, 2).is_ok());
        assert!(ensure_forward_definition_upgrade("CARD_SALES_APPROVAL", 2, 2).is_err());
        assert!(ensure_forward_definition_upgrade("CARD_SALES_APPROVAL", 2, 1).is_err());
    }
}
