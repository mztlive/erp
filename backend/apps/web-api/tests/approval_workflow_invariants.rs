//! P6-FINAL：跨阶段完整性审查、P0-D 后不变量与 20 个 DocumentType 独立验收。
//!
//! 只补集成测试与验收证据。不得修改冻结文件、领域实现、Repository、Service、
//! Handler 或前端业务组件。发现 owner 遗漏时失败关闭并退回原阶段。

use entities::document_registry::DocumentType;
use services::approval::binding::{binding_decision, BindingDecision};
use services::approval::business_adapter::{adapter_spec_of, ensure_runtime_cut_over};
use services::approval::policy::{
    policy_of, require_process_required, ApprovalDomainAction, ApprovalRequirement,
    ApprovalSubjectVersionSource, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES,
    SALES_ORDER_PROCUREMENT_CONFIRMATION,
};
use services::approval::process_kind::{document_type_of, process_kind_of};
use services::approval_codes;

/// 从生产源码截取测试模块之前的实现。
///
/// # 返回
/// 返回第一个 `#[cfg(test)]` 之前的文本；缺失时返回全文。
fn production(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// 截取指定 `pub async fn` 方法体，供创建/提交路径断言。
///
/// # 返回
/// 返回签名后到下一个 `pub async fn` 之前的文本；找不到时返回空串。
fn async_method_body<'a>(source: &'a str, signature: &str) -> &'a str {
    source
        .split(signature)
        .nth(1)
        .and_then(|body| body.split("pub async fn ").next())
        .unwrap_or("")
}

/// 断言必须审批类型的政策、适配器、cut-over 与 ProcessKind 一对一。
///
/// # 错误
/// 政策缺失、适配器缺失或映射不一致时测试失败。
fn assert_process_required_identity(
    document_type: DocumentType,
    start: ApprovalDomainAction,
    approve: ApprovalDomainAction,
    cancel: ApprovalDomainAction,
    owner_role: &str,
    version: ApprovalSubjectVersionSource,
) {
    let policy = require_process_required(document_type).expect("必须审批政策");
    assert_eq!(policy.document_type, document_type);
    assert_eq!(policy.process_kind, process_kind_of(document_type));
    assert_eq!(document_type_of(policy.process_kind), document_type);
    assert_eq!(policy.process_kind.as_str(), document_type.as_str());
    assert_eq!(policy.start_action, start);
    assert_eq!(policy.final_approve_action, approve);
    assert_eq!(policy.cancel_action, cancel);
    assert_ne!(policy.start_action, policy.final_approve_action);
    assert_ne!(policy.start_action, policy.cancel_action);
    assert_ne!(policy.final_approve_action, policy.cancel_action);
    assert_eq!(policy.work_item_owner_role.as_str(), owner_role);
    assert_eq!(policy.subject_version_source, version);
    assert!(ensure_runtime_cut_over(document_type).is_ok());
    let spec = adapter_spec_of(document_type).expect("必须登记完整适配器");
    assert_eq!(spec.document_type, document_type);
    assert_eq!(spec.on_approval_start, start);
    assert_eq!(spec.on_final_approve, approve);
    assert_eq!(spec.cancel_action, cancel);
    assert_eq!(
        binding_decision(policy_of(document_type).expect("政策").requirement()),
        BindingDecision::RequirePublished
    );
}

/// 断言无审批类型不得注册空适配器、不得启动实例。
///
/// # 错误
/// 政策不是 `NO_APPROVAL` 或仍可取得适配器时测试失败。
fn assert_no_approval_identity(document_type: DocumentType) {
    let policy = policy_of(document_type).expect("无审批政策必须存在");
    assert_eq!(policy.requirement(), ApprovalRequirement::NoApproval);
    assert_eq!(policy.process_kind(), process_kind_of(document_type));
    assert_eq!(document_type_of(policy.process_kind()), document_type);
    assert!(require_process_required(document_type).is_err());
    assert!(adapter_spec_of(document_type).is_err(), "不得注册空适配器");
    assert!(ensure_runtime_cut_over(document_type).is_ok());
    assert_eq!(
        binding_decision(policy.requirement()),
        BindingDecision::SkipNoApproval
    );
}

/// 断言创建只绑定、提交才启动。
///
/// 绑定可能下沉到同文件 helper，因此同时检查生产源码与方法体。
///
/// # 错误
/// 创建路径启动实例或提交路径未接线时测试失败。
fn assert_create_binds_submit_starts(
    source: &str,
    create_sig: &str,
    submit_sig: &str,
    submit_start_token: &str,
) {
    let create = async_method_body(source, create_sig);
    let submit = async_method_body(source, submit_sig);
    assert!(
        source.contains("bind_published_definition_on_document_create"),
        "创建必须走统一绑定端口"
    );
    assert!(!create.is_empty(), "必须定位到创建方法 {create_sig}");
    assert!(!create.contains("prepare_start("), "创建不得启动实例");
    assert!(
        !create.contains(submit_start_token),
        "创建不得调用提交启动入口 {submit_start_token}"
    );
    assert!(
        submit.contains(submit_start_token),
        "提交方法体必须调用 {submit_start_token}"
    );
    assert!(
        source.contains("prepare_start("),
        "提交链路必须最终调用 prepare_start"
    );
}

/// 断言启动读取已冻结绑定，不得按当前发布定义切换。
///
/// # 错误
/// 启动路径查询当前 PUBLISHED 或未加载绑定图时测试失败。
fn assert_start_uses_frozen_binding(start_source: &str) {
    assert!(
        start_source.contains("load_bound_definition_graph"),
        "启动必须按单据绑定加载定义图"
    );
    assert!(
        !start_source.contains("find_published_by_process_kind"),
        "启动不得改绑到当前发布版本"
    );
}

/// 断言前端升级/决定/撤回只按 allowed_actions 显示。
///
/// # 错误
/// 缺少通用动作栏或升级入口未受 allowed_actions 约束时测试失败。
fn assert_frontend_allowed_actions_only(source: &str) {
    assert_frontend_uses_generic_area(source);
    assert!(source.contains("UPGRADE_BINDING") || source.contains("allowedActions"));
}

/// 断言无审批创建走统一绑定端口且不启动实例。
///
/// # 错误
/// 创建路径启动实例或未调用统一绑定时测试失败。
fn assert_no_approval_create_skips_start(source: &str, create_sig: &str) {
    let create = async_method_body(source, create_sig);
    assert!(
        source.contains("bind_published_definition_on_document_create"),
        "无审批创建仍必须调用统一绑定端口以 SkipNoApproval"
    );
    assert!(!create.contains("prepare_start("), "无审批创建不得启动实例");
    assert!(!source.contains("ApprovalRuntimeService"));
}

/// 断言前端审批区使用通用组件且只按 `allowed_actions` 展示动作。
///
/// # 错误
/// 出现 BPM 内部符号或未接入通用动作栏时测试失败。
fn assert_frontend_uses_generic_area(source: &str) {
    assert!(source.contains("ApprovalActionBar"));
    assert!(source.contains("allowed_actions") || source.contains("allowedActions"));
    assert!(!source.contains("ProcessKind"));
    assert!(!source.contains("TransitionPlan"));
    assert!(!source.contains("plan_enter_node"));
    assert!(!source.contains("SubjectRef"));
}

/// 断言无审批页面不嵌入审批区，也不解释 BPM 内部计划。
///
/// # 错误
/// 出现审批区或 BPM 内部符号时测试失败。
fn assert_frontend_has_no_approval_area(source: &str) {
    assert!(source.contains("NO_APPROVAL"));
    assert!(!source.contains("ApprovalActionBar"));
    assert!(!source.contains("ProcessKind"));
    assert!(!source.contains("TransitionPlan"));
    assert!(!source.contains("plan_enter_node"));
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
        assert!(ensure_runtime_cut_over(document_type).is_ok());
    }
}

/// `SalesOrder` 发布必须恰有一个采购确认 purpose；其它必须审批类型不得包含。
#[test]
fn sales_order_purpose_is_exclusive_and_not_on_other_required_types() {
    let sales = require_process_required(DocumentType::SalesOrder).expect("销售必须审批");
    assert_eq!(sales.required_node_purposes.len(), 1);
    assert_eq!(
        sales.required_node_purposes[0].as_str(),
        SALES_ORDER_PROCUREMENT_CONFIRMATION
    );
    for document_type in ALL_DOCUMENT_TYPES {
        if document_type == DocumentType::SalesOrder {
            continue;
        }
        if let Ok(policy) = require_process_required(document_type) {
            assert!(
                policy.required_node_purposes.is_empty(),
                "{} 不得包含采购确认 purpose",
                document_type.as_str()
            );
        }
    }
    let definition_src = production(include_str!("../../../services/src/approval/definition.rs"));
    assert!(definition_src.contains("validate_required_purposes"));
    assert!(!definition_src.contains("if node_purpose"));
    assert!(!definition_src.contains("match purpose"));
}

/// P0-D 后全部 20 个固定类型进入新运行时；未切换失败码必须删除。
#[test]
fn all_fixed_types_enter_new_runtime_after_p0_d() {
    let mut required = 0;
    let mut no_approval = 0;
    for document_type in ALL_DOCUMENT_TYPES {
        match policy_of(document_type).expect("政策必须存在") {
            DocumentApprovalPolicy::NoApproval(_) => {
                no_approval += 1;
                assert_no_approval_identity(document_type);
            }
            DocumentApprovalPolicy::ProcessRequired(_) => {
                required += 1;
                assert!(ensure_runtime_cut_over(document_type).is_ok());
                assert!(adapter_spec_of(document_type).is_ok());
            }
        }
    }
    assert_eq!(required, 12);
    assert_eq!(no_approval, 8);
    let adapter = production(include_str!("../../../services/src/approval/business_adapter.rs"));
    let errors = production(include_str!("../../../services/src/errors.rs"));
    assert!(!adapter.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!errors.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!errors.contains("DOCUMENT_TYPE_NOT_CUT_OVER"));
    assert!(!approval_codes::ALL.contains(&"APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
}

/// 创建只绑定、提交才启动：库存调整 Service 不得在 create 路径启动实例。
#[test]
fn create_binds_and_submit_starts_for_stock_adjustment() {
    let inventory = production(include_str!("../../../services/src/inventory/mod.rs"));
    assert_create_binds_submit_starts(
        inventory,
        "async fn create_stock_adjustment",
        "async fn submit_stock_adjustment",
        "prepare_start(",
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

/// `entities::approval` 旧入口必须已由 P0-D 删除，不得再导出。
#[test]
fn entities_approval_old_entry_is_deleted() {
    let lib = include_str!("../../../entities/src/lib.rs");
    assert!(!lib.contains("pub mod approval;"));
    assert!(!lib.contains("mod approval;"));
    let stub = include_str!("../../../entities/src/approval/mod.rs");
    assert!(!stub.contains("pub use"));
    assert!(!stub.contains("pub struct"));
    assert!(!stub.contains("pub enum"));
    assert!(!stub.contains("pub fn"));
    assert!(stub.contains("旧审批运行模型已删除") || stub.contains("不再导出"));
}

/// `bpm` 只依赖 P0 allowlist；源码不得出现时钟、ID 生成或 ERP 类型。
#[test]
fn bpm_crate_stays_on_p0_allowlist() {
    let manifest = include_str!("../../../crates/bpm/Cargo.toml");
    assert!(!manifest.contains("entities"));
    assert!(!manifest.contains("database"));
    assert!(!manifest.contains("services"));
    assert!(!manifest.contains("web-api"));
    assert!(!manifest.contains("mongodb"));
    assert!(!manifest.contains("axum"));
    assert!(!manifest.contains("id-generator"));
    assert!(!manifest.contains("permission-macros"));
    let lib = include_str!("../../../crates/bpm/src/lib.rs");
    let engine = include_str!("../../../crates/bpm/src/engine/mod.rs");
    let ids = include_str!("../../../crates/bpm/src/ids.rs");
    let process_kind = include_str!("../../../crates/bpm/src/model/mod.rs");
    for source in [lib, engine, ids, process_kind] {
        let prod = production(source);
        assert!(!prod.contains("Local::now"));
        assert!(!prod.contains("Utc::now"));
        assert!(!prod.contains("SystemTime::now"));
        assert!(!prod.contains("Instant::now"));
        assert!(!prod.contains("next_id("));
        assert!(!prod.contains("DocumentType"));
        assert!(!prod.contains("WorkItem"));
        assert!(!prod.contains("Executor"));
        assert!(!prod.contains("DataScope"));
    }
}

/// `entities` 与 `services` 不得成为 BPM 流程模型或审批 ID 的第二定义源。
#[test]
fn no_second_bpm_definition_source_in_entities_or_services() {
    let entities_lib = include_str!("../../../entities/src/lib.rs");
    let entities_ids = include_str!("../../../entities/src/ids.rs");
    let integration = include_str!("../../../entities/src/approval_integration/mod.rs");
    assert!(!entities_lib.contains("pub enum ProcessKind"));
    assert!(!entities_ids.contains("ApprovalProcessInstanceId"));
    assert!(!entities_ids.contains("ApprovalProcessDefinitionId"));
    assert!(integration.contains("不得重新定义流程定义"));
    let process_kind = include_str!("../../../services/src/approval/process_kind.rs");
    assert!(process_kind.contains("pub fn process_kind_of"));
    assert!(!process_kind.contains("pub enum ProcessKind"));
    let engine = include_str!("../../../crates/bpm/src/model/mod.rs");
    assert!(engine.contains("pub enum ProcessKind"));
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
fn http_and_frontend_do_not_interpret_process_kind_or_transition_plan() {
    let instance = production(include_str!("../src/core/handler/approval_instance/mod.rs"));
    let process = production(include_str!("../src/core/handler/approval_process/mod.rs"));
    let http = production(include_str!("../src/core/handler/approval_instance/http.rs"));
    for source in [instance, process, http] {
        assert!(!source.contains("ProcessKind::"));
        assert!(!source.contains("TransitionPlan"));
        assert!(!source.contains("plan_enter_node"));
    }
    let action_bar =
        include_str!("../../../../erp-client/features/approval-workflow/components/approval-action-bar.tsx");
    let types = include_str!("../../../../erp-client/features/approval-workflow/types.ts");
    assert_frontend_uses_generic_area(action_bar);
    assert!(!types.contains("ProcessKind"));
    assert!(!types.contains("TransitionPlan"));
    assert!(types.contains("allowed_actions"));
}

/// 库存调整政策动作、资格与岗位分离符合合同。
#[test]
fn stock_adjustment_policy_actions_and_duties() {
    let policy = require_process_required(DocumentType::StockAdjustment).expect("试点政策");
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
}

/// 业务撤回要求非空原因；旧 recover / POOL / 默认办理人不得出现在目标路径。
#[test]
fn cancel_requires_reason_and_legacy_symbols_are_absent_from_target_paths() {
    let cancel = production(include_str!("../../../services/src/approval/execution/cancel.rs"));
    assert!(cancel.contains("reason"));
    let instance_mod = production(include_str!("../src/core/handler/approval_instance/mod.rs"));
    assert!(instance_mod.contains("原因不能为空"));
    let routes = production(include_str!("../src/core/routes/approval_instance.rs"));
    assert!(!routes.contains("approval_instance::recover)"));
    assert!(!routes.contains("RETRY_CURRENT_STEP"));
    assert!(routes.contains("recovery-options"));
    let work_item = production(include_str!("../../../entities/src/work_item/work_item.rs"));
    assert!(work_item.contains("DocumentApproval"));
    assert!(!work_item.contains("AssignmentMode"));
}

/// 受控升级只能到当前发布版本，且仅运行管理员、未提交未启动。
#[test]
fn upgrade_unsubmitted_only_targets_current_published_definition() {
    use services::approval::binding::{
        ensure_upgrade_unsubmitted_allowed, process_not_configured, published_definition_or_not_configured,
        APPROVAL_PROCESS_NOT_CONFIGURED,
    };

    assert!(ensure_upgrade_unsubmitted_allowed(false, false, 1, 1, 1, 1, true).is_ok());
    assert!(ensure_upgrade_unsubmitted_allowed(false, false, 1, 1, 1, 1, false).is_err());
    assert!(ensure_upgrade_unsubmitted_allowed(true, false, 1, 1, 1, 1, true).is_err());
    assert!(ensure_upgrade_unsubmitted_allowed(false, true, 1, 1, 1, 1, true).is_err());
    assert!(ensure_upgrade_unsubmitted_allowed(false, false, 2, 1, 1, 1, true).is_err());
    let missing = published_definition_or_not_configured::<()>(None).expect_err("无定义");
    assert!(missing.to_string().contains(APPROVAL_PROCESS_NOT_CONFIGURED));
    assert_eq!(
        process_not_configured().to_string(),
        format!("数据冲突: {APPROVAL_PROCESS_NOT_CONFIGURED}")
    );
    let binding = production(include_str!("../../../services/src/approval/binding.rs"));
    assert!(binding.contains("load_published_graph"));
    assert!(binding.contains("find_published_by_process_kind"));
    assert!(!binding.contains("source_definition_id"));
}

/// 创建绑定、启动进入节点和决定必须重验资格、DataScope、读取权与岗位分离。
#[test]
fn create_start_and_decide_revalidate_eligibility() {
    use services::approval::execution::authorization::{converge_eligibility, AuthorizationFailure};

    let bind = production(include_str!("../../../services/src/approval/binding.rs"));
    assert!(bind.contains("revalidate_assignee_binding_access"));
    assert!(bind.contains("ensure_separation_of_duties"));
    let auth = production(include_str!(
        "../../../services/src/approval/execution/authorization.rs"
    ));
    assert!(auth.contains("OutOfDataScope"));
    assert!(auth.contains("CannotReadSubject"));
    assert!(auth.contains("SeparationOfDuties"));
    for failure in [
        AuthorizationFailure::AccountInactive,
        AuthorizationFailure::EmploymentInvalid,
        AuthorizationFailure::NotEligible,
        AuthorizationFailure::OutOfDataScope,
        AuthorizationFailure::CannotReadSubject,
        AuthorizationFailure::SeparationOfDuties,
    ] {
        let blocked = converge_eligibility("u1", "仓储", Some(failure)).expect("资格");
        assert!(blocked.blocked_code().is_some());
    }
    let eligible = converge_eligibility("u1", "仓储", None).expect("合格");
    assert!(eligible.blocked_code().is_none());
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

/// 合同要求的 9 个 `approval_subject_version` 字段必须存在且不得回退。
#[test]
fn nine_approval_subject_version_fields_are_immutable_sources() {
    let stock = include_str!("../../../entities/src/inventory/stock_adjustment.rs");
    assert!(stock.contains("pub approval_subject_version: u32"));
    for (path, document_type) in [
        (
            include_str!("../../../entities/src/purchase_order/order.rs"),
            DocumentType::PurchaseOrder,
        ),
        (
            include_str!("../../../entities/src/purchase_order/change_order.rs"),
            DocumentType::PurchaseChangeOrder,
        ),
        (
            include_str!("../../../entities/src/receivable/customer_receipt.rs"),
            DocumentType::CustomerReceipt,
        ),
        (
            include_str!("../../../entities/src/payable/supplier_payment.rs"),
            DocumentType::SupplierPayment,
        ),
        (
            include_str!("../../../entities/src/returns/customer_refund.rs"),
            DocumentType::CustomerRefund,
        ),
        (
            include_str!("../../../entities/src/returns/supplier_refund.rs"),
            DocumentType::SupplierRefund,
        ),
        (
            include_str!("../../../entities/src/returns/receipt_reversal.rs"),
            DocumentType::ReceiptReversal,
        ),
        (
            include_str!("../../../entities/src/returns/payment_reversal.rs"),
            DocumentType::PaymentReversal,
        ),
    ] {
        let policy = require_process_required(document_type).expect("必须审批");
        assert_eq!(
            policy.subject_version_source,
            ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
        );
        assert!(path.contains("pub approval_subject_version: u32"));
        assert!(
            path.contains("approval_subject_version` 不回退")
                || path.contains("且 `approval_subject_version` 不回退"),
            "{} 撤回必须声明版本不回退",
            document_type.as_str()
        );
    }
    let stock_adapter = production(include_str!("../../../services/src/inventory/adapter.rs"));
    assert!(stock_adapter.contains("成功后不回退"));
    assert!(stock_adapter.contains("subject_version` 不回退"));
    assert_eq!(
        require_process_required(DocumentType::StockAdjustment)
            .expect("试点")
            .subject_version_source,
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion
    );
    let sales = require_process_required(DocumentType::SalesOrder).expect("销售");
    let voucher = require_process_required(DocumentType::VoucherSalesOrder).expect("卡券销售");
    let change = require_process_required(DocumentType::SalesChangeOrder).expect("销售变更");
    assert_eq!(
        sales.subject_version_source,
        ApprovalSubjectVersionSource::SalesOrderSubmissionNo
    );
    assert_eq!(
        voucher.subject_version_source,
        ApprovalSubjectVersionSource::SalesOrderSubmissionNo
    );
    assert_eq!(
        change.subject_version_source,
        ApprovalSubjectVersionSource::SalesChangeSubmissionNo
    );
}

/// SalesOrder 独立验收：创建绑定、提交启动、唯一采购确认 purpose、通用审批区。
#[test]
fn sales_order_acceptance_record() {
    assert_process_required_identity(
        DocumentType::SalesOrder,
        ApprovalDomainAction::SalesOrderStartApprovalSubmission,
        ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission,
        ApprovalDomainAction::SalesOrderCancelApprovalSubmission,
        "sales_order_approver",
        ApprovalSubjectVersionSource::SalesOrderSubmissionNo,
    );
    let command = production(include_str!("../../../services/src/sales_order/command.rs"));
    let create = async_method_body(command, "async fn create_sales_order");
    let submit = async_method_body(command, "async fn submit_sales_order");
    assert!(
        command.contains("bind_published_definition_on_document_create"),
        "销售创建必须走统一绑定端口"
    );
    assert!(
        create.contains("persist_bound_sales_document"),
        "创建只绑定，不得在方法内启动实例"
    );
    assert!(!create.contains("prepare_start("), "创建不得启动实例");
    assert!(submit.contains("prepare_start("), "提交必须启动");
    assert!(command.contains("document_type_for_sales_create"));
    let start = production(include_str!(
        "../../../services/src/sales_order/start_approval.rs"
    ));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        require_process_required(DocumentType::SalesOrder)
            .expect("销售")
            .final_approve_action
            .as_str(),
        "SalesOrderService::formalize_approved_submission"
    );
    let frontend =
        include_str!("../../../../erp-client/features/sales-orders/components/sales-order-approval-area.tsx");
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("SALES_ORDER_DOCUMENT_TYPE"));
}

/// VoucherSalesOrder 独立验收：与 SalesOrder 分派、独立 ProcessKind、无采购确认 purpose。
#[test]
fn voucher_sales_order_acceptance_record() {
    assert_process_required_identity(
        DocumentType::VoucherSalesOrder,
        ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission,
        ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission,
        ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission,
        "voucher_sales_order_approver",
        ApprovalSubjectVersionSource::SalesOrderSubmissionNo,
    );
    let sales = require_process_required(DocumentType::SalesOrder).expect("销售");
    let voucher = require_process_required(DocumentType::VoucherSalesOrder).expect("卡券销售");
    assert_ne!(sales.process_kind, voucher.process_kind);
    assert!(voucher.required_node_purposes.is_empty());
    let command = production(include_str!("../../../services/src/sales_order/command.rs"));
    assert!(command.contains("BusinessType::Voucher"));
    let create = async_method_body(command, "async fn create_sales_order");
    let submit = async_method_body(command, "async fn submit_sales_order");
    assert!(!create.contains("prepare_start("));
    assert!(submit.contains("prepare_start("));
    let start = production(include_str!(
        "../../../services/src/sales_order/start_approval.rs"
    ));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        voucher.final_approve_action.as_str(),
        "SalesOrderService::formalize_approved_submission"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/sales-orders/components/voucher-sales-order-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("VOUCHER_SALES_ORDER_DOCUMENT_TYPE"));
}

/// SalesChangeOrder 独立验收：创建绑定、提交启动、变更提交版本。
#[test]
fn sales_change_order_acceptance_record() {
    assert_process_required_identity(
        DocumentType::SalesChangeOrder,
        ApprovalDomainAction::SalesChangeOrderSubmitSalesChange,
        ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange,
        ApprovalDomainAction::SalesChangeOrderCancelApproval,
        "sales_change_order_approver",
        ApprovalSubjectVersionSource::SalesChangeSubmissionNo,
    );
    let source = production(include_str!(
        "../../../services/src/sales_review/sales_change_order.rs"
    ));
    assert_create_binds_submit_starts(
        source,
        "async fn create_sales_change_order",
        "async fn submit_sales_change",
        "start_change_approval(",
    );
    let start = production(include_str!(
        "../../../services/src/sales_review/start_approval.rs"
    ));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        require_process_required(DocumentType::SalesChangeOrder)
            .expect("销售变更")
            .final_approve_action
            .as_str(),
        "SalesChangeOrderService::apply_effective_change"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/sales-orders/components/sales-change-order-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("SALES_CHANGE_ORDER_DOCUMENT_TYPE"));
}

/// PurchaseOrder 独立验收：提交启动、实体版本、无采购确认 purpose。
#[test]
fn purchase_order_acceptance_record() {
    assert_process_required_identity(
        DocumentType::PurchaseOrder,
        ApprovalDomainAction::PurchaseOrderSubmit,
        ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder,
        ApprovalDomainAction::PurchaseOrderCancelApproval,
        "purchase_order_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let submit = production(include_str!("../../../services/src/purchase_order/submission.rs"));
    let submit_body = async_method_body(submit, "async fn submit(");
    assert!(submit_body.contains("prepare_start("));
    assert!(submit_body.contains("load_bound_definition_graph"));
    let start = production(include_str!(
        "../../../services/src/purchase_order/start_approval.rs"
    ));
    assert_start_uses_frozen_binding(start);
    let create = production(include_str!(
        "../../../services/src/purchase_order/creation_basis.rs"
    ));
    assert!(create.contains("采购创建依据不存在"));
    assert!(!create.contains("bind_published_definition_on_document_create"));
    let draft = production(include_str!("../../../services/src/purchase_order/draft_edit.rs"));
    assert!(!draft.contains("bind_published_definition_on_document_create"));
    assert!(!draft.contains("prepare_start("));
    assert_eq!(
        require_process_required(DocumentType::PurchaseOrder)
            .expect("采购")
            .final_approve_action
            .as_str(),
        "PurchaseOrderService::formalize_approved_order"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/purchase-orders/components/purchase-order-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("PURCHASE_ORDER_DOCUMENT_TYPE"));
}

/// PurchaseChangeOrder 独立验收：创建绑定、提交启动。
#[test]
fn purchase_change_order_acceptance_record() {
    assert_process_required_identity(
        DocumentType::PurchaseChangeOrder,
        ApprovalDomainAction::PurchaseChangeOrderSubmitChange,
        ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
        ApprovalDomainAction::PurchaseChangeOrderCancelApproval,
        "purchase_change_order_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/purchase_order/change.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn start_change",
        "async fn submit_change",
        "start_change_approval(",
    );
    let start = production(include_str!(
        "../../../services/src/purchase_order/change_start.rs"
    ));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        require_process_required(DocumentType::PurchaseChangeOrder)
            .expect("采购变更")
            .final_approve_action
            .as_str(),
        "PurchaseChangeService::apply_effective_change"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/purchase-orders/components/purchase-change-order-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE"));
}

/// StockAdjustment 独立验收：试点类型走实体版本与数量快照。
#[test]
fn stock_adjustment_acceptance_record() {
    assert_process_required_identity(
        DocumentType::StockAdjustment,
        ApprovalDomainAction::StockAdjustmentSubmit,
        ApprovalDomainAction::StockAdjustmentPost,
        ApprovalDomainAction::StockAdjustmentCancelApproval,
        "stock_adjustment_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let inventory = production(include_str!("../../../services/src/inventory/mod.rs"));
    assert_create_binds_submit_starts(
        inventory,
        "async fn create_stock_adjustment",
        "async fn submit_stock_adjustment",
        "prepare_start(",
    );
    let start = production(include_str!("../../../services/src/inventory/start_approval.rs"));
    assert_start_uses_frozen_binding(start);
    let policy = require_process_required(DocumentType::StockAdjustment).expect("试点");
    assert!(policy.required_node_purposes.is_empty());
    assert_eq!(
        policy.final_approve_action.as_str(),
        "InventoryService::post_stock_adjustment"
    );
    let frontend =
        include_str!("../../../../erp-client/features/inventory/components/adjustment-approval-area.tsx");
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("STOCK_ADJUSTMENT_DOCUMENT_TYPE"));
}

/// CustomerReceipt 独立验收：资金类金额快照、创建绑定、提交启动。
#[test]
fn customer_receipt_acceptance_record() {
    assert_process_required_identity(
        DocumentType::CustomerReceipt,
        ApprovalDomainAction::CustomerReceiptSubmit,
        ApprovalDomainAction::CustomerReceiptPost,
        ApprovalDomainAction::CustomerReceiptCancelApproval,
        "customer_receipt_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/receivable/mod.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn create_customer_receipt",
        "async fn submit_customer_receipt",
        "dispatch_customer_receipt_start(",
    );
    let start = production(include_str!("../../../services/src/receivable/start_approval.rs"));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        require_process_required(DocumentType::CustomerReceipt)
            .expect("回款")
            .final_approve_action
            .as_str(),
        "ReceivableService::post_customer_receipt"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/customer-receivables/components/customer-receipt-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("CUSTOMER_RECEIPT_DOCUMENT_TYPE"));
}

/// SupplierPayment 独立验收：资金类金额快照、创建绑定、提交启动。
#[test]
fn supplier_payment_acceptance_record() {
    assert_process_required_identity(
        DocumentType::SupplierPayment,
        ApprovalDomainAction::SupplierPaymentSubmit,
        ApprovalDomainAction::SupplierPaymentPost,
        ApprovalDomainAction::SupplierPaymentCancelApproval,
        "supplier_payment_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/payable/mod.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn create_supplier_payment",
        "async fn submit_supplier_payment",
        "dispatch_supplier_payment_start(",
    );
    let start = production(include_str!("../../../services/src/payable/start_approval.rs"));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        require_process_required(DocumentType::SupplierPayment)
            .expect("付款")
            .final_approve_action
            .as_str(),
        "PayableService::post_supplier_payment"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/supplier-payables/components/supplier-payment-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("SUPPLIER_PAYMENT_DOCUMENT_TYPE"));
}

/// CustomerRefund 独立验收：退款提交启动、通用审批区。
#[test]
fn customer_refund_acceptance_record() {
    assert_process_required_identity(
        DocumentType::CustomerRefund,
        ApprovalDomainAction::CustomerRefundSubmit,
        ApprovalDomainAction::CustomerRefundPost,
        ApprovalDomainAction::CustomerRefundCancelApproval,
        "customer_refund_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/returns/customer_refund.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn create_customer_refund",
        "async fn submit_customer_refund",
        "dispatch_customer_refund_start(",
    );
    let start = production(include_str!("../../../services/src/returns/start_approval.rs"));
    assert_start_uses_frozen_binding(start);
    assert_eq!(
        require_process_required(DocumentType::CustomerRefund)
            .expect("客户退款")
            .final_approve_action
            .as_str(),
        "ReturnsService::post_customer_refund"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/customer-receivables/components/customer-refund-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("CUSTOMER_REFUND_DOCUMENT_TYPE"));
}

/// SupplierRefund 独立验收：供应商退款提交启动。
#[test]
fn supplier_refund_acceptance_record() {
    assert_process_required_identity(
        DocumentType::SupplierRefund,
        ApprovalDomainAction::SupplierRefundSubmit,
        ApprovalDomainAction::SupplierRefundPost,
        ApprovalDomainAction::SupplierRefundCancelApproval,
        "supplier_refund_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/returns/supplier_refund.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn create_supplier_refund",
        "async fn submit_supplier_refund",
        "dispatch_supplier_refund_start(",
    );
    assert_eq!(
        require_process_required(DocumentType::SupplierRefund)
            .expect("供应商退款")
            .final_approve_action
            .as_str(),
        "ReturnsService::post_supplier_refund"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/supplier-payables/components/supplier-refund-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("SUPPLIER_REFUND_DOCUMENT_TYPE"));
}

/// ReceiptReversal 独立验收：回款冲正提交启动。
#[test]
fn receipt_reversal_acceptance_record() {
    assert_process_required_identity(
        DocumentType::ReceiptReversal,
        ApprovalDomainAction::ReceiptReversalSubmit,
        ApprovalDomainAction::ReceiptReversalPost,
        ApprovalDomainAction::ReceiptReversalCancelApproval,
        "receipt_reversal_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/returns/receipt_reversal.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn create_receipt_reversal",
        "async fn submit_receipt_reversal",
        "dispatch_receipt_reversal_start(",
    );
    assert_eq!(
        require_process_required(DocumentType::ReceiptReversal)
            .expect("回款冲正")
            .final_approve_action
            .as_str(),
        "ReturnsService::post_receipt_reversal"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/customer-receivables/components/receipt-reversal-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("RECEIPT_REVERSAL_DOCUMENT_TYPE"));
}

/// PaymentReversal 独立验收：付款冲正提交启动。
#[test]
fn payment_reversal_acceptance_record() {
    assert_process_required_identity(
        DocumentType::PaymentReversal,
        ApprovalDomainAction::PaymentReversalSubmit,
        ApprovalDomainAction::PaymentReversalPost,
        ApprovalDomainAction::PaymentReversalCancelApproval,
        "payment_reversal_approver",
        ApprovalSubjectVersionSource::EntityApprovalSubjectVersion,
    );
    let source = production(include_str!("../../../services/src/returns/payment_reversal.rs"));
    assert_create_binds_submit_starts(
        source,
        "async fn create_payment_reversal",
        "async fn submit_payment_reversal",
        "dispatch_payment_reversal_start(",
    );
    assert_eq!(
        require_process_required(DocumentType::PaymentReversal)
            .expect("付款冲正")
            .final_approve_action
            .as_str(),
        "ReturnsService::post_payment_reversal"
    );
    let frontend = include_str!(
        "../../../../erp-client/features/supplier-payables/components/payment-reversal-approval-area.tsx"
    );
    assert_frontend_allowed_actions_only(frontend);
    assert!(frontend.contains("PAYMENT_REVERSAL_DOCUMENT_TYPE"));
}

/// PurchaseReceipt 独立验收：无绑定、无实例、无空适配器。
#[test]
fn purchase_receipt_acceptance_record() {
    assert_no_approval_identity(DocumentType::PurchaseReceipt);
    let source = production(include_str!(
        "../../../services/src/fulfillment/purchase_receipt.rs"
    ));
    assert_no_approval_create_skips_start(source, "async fn create_purchase_receipt");
    let frontend = include_str!(
        "../../../../erp-client/features/fulfillment-operations/lib/purchase-receipt-no-approval.ts"
    );
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("PURCHASE_RECEIPT_DOCUMENT_TYPE"));
    assert!(frontend.contains("PURCHASE_RECEIPT_APPROVAL_REQUIREMENT"));
}

/// Delivery 独立验收：仓发创建不启动审批。
#[test]
fn delivery_acceptance_record() {
    assert_no_approval_identity(DocumentType::Delivery);
    let source = production(include_str!("../../../services/src/fulfillment/delivery.rs"));
    assert_no_approval_create_skips_start(source, "async fn create_delivery");
    let frontend =
        include_str!("../../../../erp-client/features/fulfillment-operations/lib/delivery-no-approval.ts");
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("DELIVERY_DOCUMENT_TYPE"));
    assert!(frontend.contains("DELIVERY_APPROVAL_REQUIREMENT"));
}

/// ElectronicDelivery 独立验收：电子交付创建不启动审批。
#[test]
fn electronic_delivery_acceptance_record() {
    assert_no_approval_identity(DocumentType::ElectronicDelivery);
    let source = production(include_str!(
        "../../../services/src/fulfillment/electronic_delivery.rs"
    ));
    assert_no_approval_create_skips_start(source, "async fn create_electronic_delivery");
    let frontend = include_str!(
        "../../../../erp-client/features/fulfillment-operations/lib/electronic-delivery-no-approval.ts"
    );
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("ELECTRONIC_DELIVERY_DOCUMENT_TYPE"));
    assert!(frontend.contains("ELECTRONIC_DELIVERY_APPROVAL_REQUIREMENT"));
}

/// ServiceFulfillment 独立验收：服务履约创建不启动审批。
#[test]
fn service_fulfillment_acceptance_record() {
    assert_no_approval_identity(DocumentType::ServiceFulfillment);
    let source = production(include_str!(
        "../../../services/src/fulfillment/service_fulfillment.rs"
    ));
    assert_no_approval_create_skips_start(source, "async fn create_service_fulfillment");
    let frontend = include_str!(
        "../../../../erp-client/features/fulfillment-operations/lib/service-fulfillment-no-approval.ts"
    );
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("SERVICE_FULFILLMENT_DOCUMENT_TYPE"));
    assert!(frontend.contains("SERVICE_FULFILLMENT_APPROVAL_REQUIREMENT"));
}

/// CustomerAcceptance 独立验收：客户验收创建不启动审批。
#[test]
fn customer_acceptance_acceptance_record() {
    assert_no_approval_identity(DocumentType::CustomerAcceptance);
    let source = production(include_str!(
        "../../../services/src/fulfillment/customer_acceptance.rs"
    ));
    assert_no_approval_create_skips_start(source, "async fn create_customer_acceptance");
    let frontend = include_str!(
        "../../../../erp-client/features/fulfillment-operations/lib/customer-acceptance-no-approval.ts"
    );
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE"));
    assert!(frontend.contains("CUSTOMER_ACCEPTANCE_APPROVAL_REQUIREMENT"));
}

/// Invoice 独立验收：发票创建不绑定流程、无提交启动。
#[test]
fn invoice_acceptance_record() {
    assert_no_approval_identity(DocumentType::Invoice);
    let source = production(include_str!("../../../services/src/receivable/mod.rs"));
    let create = async_method_body(source, "async fn create_invoice");
    assert!(source.contains("bind_published_definition_on_document_create"));
    assert!(!create.contains("prepare_start("));
    assert!(!source.contains("pub async fn submit_invoice"));
    let frontend =
        include_str!("../../../../erp-client/features/customer-receivables/lib/invoice-no-approval.ts");
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("INVOICE_DOCUMENT_TYPE"));
    assert!(frontend.contains("INVOICE_APPROVAL_REQUIREMENT"));
}

/// SalesReturnCase 独立验收：销售退货创建不启动审批。
#[test]
fn sales_return_case_acceptance_record() {
    assert_no_approval_identity(DocumentType::SalesReturnCase);
    let source = production(include_str!("../../../services/src/returns/sales_return.rs"));
    assert_no_approval_create_skips_start(source, "async fn create_sales_return_case");
    let frontend =
        include_str!("../../../../erp-client/features/sales-orders/lib/sales-return-no-approval.ts");
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("SALES_RETURN_CASE_DOCUMENT_TYPE"));
    assert!(frontend.contains("SALES_RETURN_CASE_APPROVAL_REQUIREMENT"));
}

/// PurchaseReturnOrder 独立验收：采购退货创建不启动审批。
#[test]
fn purchase_return_order_acceptance_record() {
    assert_no_approval_identity(DocumentType::PurchaseReturnOrder);
    let source = production(include_str!("../../../services/src/returns/purchase_return.rs"));
    assert_no_approval_create_skips_start(source, "async fn create_purchase_return_order");
    let frontend = include_str!(
        "../../../../erp-client/features/purchase-orders/lib/purchase-return-order-no-approval.ts"
    );
    assert_frontend_has_no_approval_area(frontend);
    assert!(frontend.contains("PURCHASE_RETURN_ORDER_DOCUMENT_TYPE"));
    assert!(frontend.contains("PURCHASE_RETURN_ORDER_APPROVAL_REQUIREMENT"));
}
