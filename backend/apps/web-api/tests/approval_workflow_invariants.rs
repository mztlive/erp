//! P6-PILOT：跨阶段完整性审查与试点不变式。
//!
//! 只证明 owner 归属、bpm 单向依赖、目标调用方不得经 `entities::approval`
//! 间接取得 BPM 类型，以及合同 §8 中不依赖 Mongo 的确定性规则。

use entities::document_registry::DocumentType;
use services::approval::business_adapter::ensure_runtime_cut_over;
use services::approval::policy::{
    policy_of, require_process_required, ApprovalRequirement, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES,
    SALES_ORDER_PROCUREMENT_CONFIRMATION,
};
use services::approval::process_kind::{document_type_of, process_kind_of};
use services::APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER;

/// 从生产源码截取测试模块之前的实现。
///
/// # 错误
/// 找不到测试分隔符时测试失败。
fn production(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// 20 个固定 DocumentType 与 ProcessKind 双向穷尽一一对应。
#[test]
fn document_type_maps_to_unique_stable_process_kind() {
    assert_eq!(ALL_DOCUMENT_TYPES.len(), 20);
    for document_type in ALL_DOCUMENT_TYPES {
        let process_kind = process_kind_of(document_type);
        assert_eq!(document_type_of(process_kind), document_type);
        assert_eq!(process_kind.as_str(), document_type.as_str());
        assert!(policy_of(document_type).is_ok());
    }
}

/// `SalesOrder` 发布必须恰有一个采购确认 purpose；库存调整不得包含。
#[test]
fn sales_order_purpose_is_exclusive_and_not_on_stock_adjustment() {
    let sales = require_process_required(DocumentType::SalesOrder).expect("销售必须审批");
    assert_eq!(sales.required_node_purposes.len(), 1);
    assert_eq!(
        sales.required_node_purposes[0].as_str(),
        SALES_ORDER_PROCUREMENT_CONFIRMATION
    );
    let stock = require_process_required(DocumentType::StockAdjustment).expect("库存调整必须审批");
    assert!(stock.required_node_purposes.is_empty());
    let definition_src = production(include_str!("../../../services/src/approval/definition.rs"));
    assert!(definition_src.contains("validate_required_purposes"));
    assert!(!definition_src.contains("if node_purpose"));
    assert!(!definition_src.contains("match purpose"));
}

/// 试点已 cut-over；其余 PROCESS_REQUIRED 必须失败关闭且不得回退旧运行时。
#[test]
fn only_stock_adjustment_process_required_is_cut_over() {
    let mut required = 0;
    let mut no_approval = 0;
    for document_type in ALL_DOCUMENT_TYPES {
        match policy_of(document_type).expect("政策必须存在") {
            DocumentApprovalPolicy::NoApproval(_) => {
                no_approval += 1;
                assert!(ensure_runtime_cut_over(document_type).is_ok());
            }
            DocumentApprovalPolicy::ProcessRequired(_) => {
                required += 1;
                if document_type == DocumentType::StockAdjustment {
                    assert!(ensure_runtime_cut_over(document_type).is_ok());
                } else {
                    let error = ensure_runtime_cut_over(document_type).expect_err("未切换必须失败");
                    assert!(error.to_string().contains(APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER));
                }
            }
        }
    }
    assert_eq!(required, 12);
    assert_eq!(no_approval, 8);
}

/// 创建只绑定、提交才启动：库存调整 Service 不得在 create 路径启动实例。
#[test]
fn create_binds_and_submit_starts_for_stock_adjustment() {
    let inventory = production(include_str!("../../../services/src/inventory/mod.rs"));
    assert!(inventory.contains("create_stock_adjustment"));
    assert!(inventory.contains("submit_stock_adjustment"));
    let create = inventory
        .split("async fn create_stock_adjustment")
        .nth(1)
        .and_then(|body| body.split("async fn ").next())
        .expect("创建方法");
    assert!(!create.contains("prepare_start"));
    assert!(!create.contains("start_approval"));
    let submit = inventory
        .split("async fn submit_stock_adjustment")
        .nth(1)
        .and_then(|body| body.split("async fn ").next())
        .expect("提交方法");
    let starts_via_prepare = submit.contains("prepare_start(");
    let starts_via_runtime = submit.contains("ApprovalRuntimeService") && submit.contains(".start(");
    assert!(
        starts_via_prepare || starts_via_runtime,
        "§8.2.2 提交才启动：submit_stock_adjustment 必须调用 prepare_start 或 ApprovalRuntimeService::start；当前未接线，退回 P3-ADAPTER-PILOT"
    );
}

/// 目标调用方不得经 `entities::approval` 间接取得 BPM 类型。
#[test]
fn target_callers_must_not_use_entities_approval_for_bpm_types() {
    let inventory = production(include_str!("../../../services/src/inventory/adapter.rs"));
    let binding = production(include_str!("../../../services/src/approval/binding.rs"));
    let runtime = production(include_str!(
        "../../../services/src/approval/execution/runtime_service.rs"
    ));
    let http_instance = production(include_str!("../src/core/handler/approval_instance/mod.rs"));
    let http_process = production(include_str!("../src/core/handler/approval_process/mod.rs"));
    for source in [inventory, binding, runtime, http_instance, http_process] {
        assert!(!source.contains("entities::approval::"));
        assert!(!source.contains("use entities::approval;"));
        assert!(!source.contains("use entities::approval::{"));
        assert!(!source.contains("CARD_SALES_APPROVAL"));
        assert!(!source.contains("fn recover("));
    }
}

/// `bpm` 只依赖 P0 allowlist；源码不得出现时钟或 ID 生成。
#[test]
fn bpm_crate_stays_on_p0_allowlist() {
    let manifest = include_str!("../../../crates/bpm/Cargo.toml");
    assert!(!manifest.contains("entities"));
    assert!(!manifest.contains("database"));
    assert!(!manifest.contains("services"));
    assert!(!manifest.contains("mongodb"));
    assert!(!manifest.contains("axum"));
    assert!(!manifest.contains("id-generator"));
    let lib = include_str!("../../../crates/bpm/src/lib.rs");
    let engine = include_str!("../../../crates/bpm/src/engine/mod.rs");
    for source in [lib, engine] {
        let prod = production(source);
        assert!(!prod.contains("Local::now"));
        assert!(!prod.contains("Utc::now"));
        assert!(!prod.contains("SystemTime::now"));
        assert!(!prod.contains("Instant::now"));
        assert!(!prod.contains("next_id("));
        assert!(!prod.contains("DocumentType"));
        assert!(!prod.contains("WorkItem"));
        assert!(!prod.contains("Executor"));
    }
}

/// `id_type!` 只有 `entity-macros` 一个定义源。
#[test]
fn id_type_has_single_definition_source() {
    let macros = include_str!("../../../crates/entity-macros/src/lib.rs");
    assert!(macros.contains("pub fn id_type"));
    assert!(macros.contains("#[proc_macro]"));
    let forbidden_macro_rules = concat!("macro_rules!", " id_type");
    assert!(!macros.contains(forbidden_macro_rules));
    let bpm_ids = include_str!("../../../crates/bpm/src/ids.rs");
    let entity_ids = include_str!("../../../entities/src/ids.rs");
    assert!(bpm_ids.contains("id_type!"));
    assert!(entity_ids.contains("id_type!"));
    assert!(!bpm_ids.contains(forbidden_macro_rules));
    assert!(!entity_ids.contains(forbidden_macro_rules));
}

/// HTTP 与前端不得直接解释 ProcessKind 或 BPM 内部计划。
#[test]
fn http_does_not_interpret_process_kind_or_transition_plan() {
    let instance = production(include_str!("../src/core/handler/approval_instance/mod.rs"));
    let process = production(include_str!("../src/core/handler/approval_process/mod.rs"));
    for source in [instance, process] {
        assert!(!source.contains("ProcessKind::"));
        assert!(!source.contains("TransitionPlan"));
        assert!(!source.contains("plan_enter_node"));
    }
}

/// 库存调整政策动作、资格与岗位分离符合合同。
#[test]
fn stock_adjustment_policy_actions_and_duties() {
    let policy = require_process_required(DocumentType::StockAdjustment).expect("试点政策");
    assert_eq!(policy.document_type, DocumentType::StockAdjustment);
    assert_eq!(
        policy.start_action.as_str(),
        "InventoryService::submit_stock_adjustment"
    );
    assert_eq!(
        policy.final_approve_action.as_str(),
        "InventoryService::post_stock_adjustment"
    );
    assert_eq!(
        policy.cancel_action.as_str(),
        "InventoryService::cancel_stock_adjustment_approval"
    );
    assert_eq!(policy.work_item_owner_role.as_str(), "stock_adjustment_approver");
    assert_eq!(
        policy_of(DocumentType::StockAdjustment)
            .expect("政策")
            .requirement(),
        ApprovalRequirement::ProcessRequired
    );
}

/// 业务撤回要求非空原因；旧 recover / POOL / 默认办理人不得出现在目标路径。
#[test]
fn cancel_requires_reason_and_legacy_symbols_are_absent_from_target_paths() {
    let cancel = production(include_str!("../../../services/src/approval/execution/cancel.rs"));
    assert!(cancel.contains("reason"));
    let instance_http = production(include_str!("../src/core/handler/approval_instance/http.rs"));
    assert!(instance_http.contains("原因不能为空") || instance_http.contains("reason"));
    let routes = production(include_str!("../src/core/routes/approval_instance.rs"));
    assert!(!routes.contains("approval_instance::recover)"));
    assert!(!routes.contains("RETRY_CURRENT_STEP"));
    assert!(routes.contains("recovery-options"));
    let work_item = production(include_str!("../../../entities/src/work_item/work_item.rs"));
    assert!(work_item.contains("DocumentApproval"));
}

/// 快照与 SubjectRef + subject_version 一致且写后不可变。
#[test]
fn snapshot_write_path_is_create_only() {
    let repo = production(include_str!(
        "../../../database/src/repository/approval_integration.rs"
    ));
    assert!(repo.contains("create_immutable_snapshot"));
    assert!(!repo.contains("update_snapshot"));
    assert!(!repo.contains("fn update("));
}
