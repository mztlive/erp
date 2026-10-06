//! 业务 audit_logs 的显式动作目录；身份 audit_events 的目录由身份域单独维护。

use crate::{AuditAction, AuditCode, AuditField, AuditFieldKind, Error, Result};

const fn action(code: &'static str, resource_type: &'static str, label: &'static str) -> AuditAction {
    AuditAction { code, resource_type, label, version: 1, allowed_fields: &[] }
}

const fn portal_action(code: &'static str, label: &'static str) -> AuditAction {
    AuditAction {
        code,
        resource_type: "supplier_portal_request",
        label,
        version: 1,
        allowed_fields: &[
            AuditField { code: "available_quantity", label: "可供数量", kind: AuditFieldKind::Quantity },
            AuditField {
                code: "before_available_quantity",
                label: "原可供数量",
                kind: AuditFieldKind::Quantity,
            },
            AuditField {
                code: "after_available_quantity",
                label: "本次可供数量",
                kind: AuditFieldKind::Quantity,
            },
            AuditField {
                code: "quantity_reported",
                label: "数量报送状态",
                kind: AuditFieldKind::Code(&[
                    AuditCode { code: "PROVIDED", label: "已提供" },
                    AuditCode { code: "NOT_PROVIDED", label: "未提供" },
                ]),
            },
            AuditField {
                code: "availability_status",
                label: "可供状态",
                kind: AuditFieldKind::Code(&[
                    AuditCode { code: "AVAILABLE", label: "可供" },
                    AuditCode { code: "UNAVAILABLE", label: "不可供" },
                    AuditCode { code: "STOPPED", label: "停止供应" },
                    AuditCode { code: "STALE", label: "数据已过期" },
                ]),
            },
        ],
    }
}

/// 普通安全事件默认不记录任意字段；具有专用投影的类型化入口保留自身白名单。
const REGISTERED_ACTIONS: &[AuditAction] = &[
    portal_action("supplier_portal.account_create", "开通供应商门户账号"),
    portal_action("supplier_portal.account_update", "调整供应商门户账号"),
    portal_action("supplier_portal.password_update", "供应商修改密码"),
    portal_action("supplier_portal.asset_register", "供应商登记申请素材"),
    portal_action("supplier_portal.availability_update", "供应商更新可供"),
    portal_action("supplier_portal.application_save", "保存供应商报价申请"),
    portal_action("supplier_portal.application_submit", "提交供应商报价申请"),
    portal_action("supplier_portal.application_withdraw", "撤回供应商报价申请"),
    portal_action("supplier_portal.application_approve", "确认供应商报价申请"),
    portal_action("supplier_portal.application_return", "退回供应商报价申请"),
    portal_action("supplier_portal.quote_access_update", "调整供应商报价开放目录"),
    portal_action("supplier_portal.new_product_save", "保存供应商新品提报"),
    portal_action("supplier_portal.new_product_submit", "提交供应商新品提报"),
    portal_action("supplier_portal.new_product_withdraw", "撤回供应商新品提报"),
    portal_action("supplier_portal.new_product_approve", "确认供应商新品提报"),
    portal_action("supplier_portal.new_product_return", "退回供应商新品提报"),
    portal_action("supplier_portal.commercial_save", "保存供应商合作条款申请"),
    portal_action("supplier_portal.commercial_submit", "提交供应商合作条款申请"),
    portal_action("supplier_portal.commercial_withdraw", "撤回供应商合作条款申请"),
    portal_action("supplier_portal.commercial_approve", "确认供应商合作条款申请"),
    portal_action("supplier_portal.commercial_return", "退回供应商合作条款申请"),
    action("admin.create", "admin", "创建管理员账号"),
    action("admin.delete", "admin", "删除管理员账号"),
    action("admin.role.update", "admin", "修改管理员角色"),
    action("admin.update", "admin", "修改管理员账号"),
    action("approval.cancel_blocked", "approval_process_instance", "取消阻塞审批"),
    action("approval.decide", "approval_process_instance", "登记审批决定"),
    action("approval.definition.bound", "business_document", "绑定单据审批定义"),
    action("approval.definition.policy", "business_document", "登记单据审批政策"),
    action("approval.definition.upgraded", "business_document", "升级单据审批定义"),
    action("approval.resume_current_approver", "approval_process_instance", "恢复当前审批人"),
    action("approval_definition.create_draft", "approval_process_definition", "创建审批定义草稿"),
    action("approval_definition.publish", "approval_process_definition", "发布审批定义"),
    action("approval_definition.replace_nodes", "approval_process_definition", "修改审批定义节点"),
    action("approval_definition.retire", "approval_process_definition", "停用审批定义"),
    action("auth.login", "auth", "登录系统"),
    action("background_job.cancel", "background_job", "取消后台任务"),
    action("background_job.cancel_all", "background_job", "取消后台任务全部待处理事项"),
    action("background_job.create", "background_job", "创建后台任务"),
    action("bulk_selection_snapshot.confirm", "bulk_selection_snapshot", "确认批量选择快照"),
    action("bulk_selection_snapshot.create", "bulk_selection_snapshot", "创建批量选择快照"),
    action("bulk_selection_snapshot.expire", "bulk_selection_snapshot", "使批量选择快照失效"),
    action("business_document.register", "business_document", "登记业务单据"),
    action("contract.archive_revision", "contract", "归档版本合同"),
    action("contract.create", "contract", "创建合同"),
    action("contract.terminate", "contract", "终止合同"),
    action("contract_application.apply", "contract_application", "应用合同申请"),
    action("contract_application.download", "contract_application", "下载合同申请"),
    action("contract_number_counter.configure", "contract_number_counter", "配置合同编号规则"),
    action("contract_template.create", "contract_template", "创建合同模板"),
    action("contract_template.sample", "contract_template", "生成样例合同模板"),
    action("contract_template.status", "contract_template", "变更状态合同模板"),
    action("cost_entry.create", "cost_entry", "登记费用"),
    action("customer.create", "customer", "创建客户"),
    action("customer.delete", "customer", "删除客户"),
    action("customer.update", "customer", "修改客户"),
    action("customer_acceptance.commit", "customer_acceptance", "登记并确认客户验收"),
    action("customer_acceptance.create", "customer_acceptance", "创建客户验收单"),
    action("customer_acceptance.post", "customer_acceptance", "确认客户验收"),
    action("customer_acceptance.reverse", "customer_acceptance", "冲正客户验收"),
    action("customer_assignment.assign", "customer_assignment", "分配客户归属"),
    action("customer_assignment.end", "customer_assignment", "结束客户归属"),
    action("customer_profile.create", "customer_profile", "创建客户档案"),
    action("customer_profile.update", "customer_profile", "修改客户档案"),
    action("customer_receipt.cancel_approval", "customer_receipt", "撤回客户回款审批"),
    action("customer_receipt.commit", "customer_receipt", "登记并提交客户回款"),
    action("customer_receipt.create", "customer_receipt", "登记客户回款"),
    action("customer_receipt.post", "customer_receipt", "过账客户回款"),
    action("customer_receipt.submit", "customer_receipt", "提交客户回款"),
    action("customer_receipt.update", "customer_receipt", "修改客户回款"),
    action("customer_refund.cancel_approval", "customer_refund", "撤回审批客户退款单"),
    action("customer_refund.create", "customer_refund", "创建客户退款单"),
    action("customer_refund.post", "customer_refund", "过账客户退款单"),
    action("customer_refund.submit", "customer_refund", "提交审批客户退款单"),
    action("customer_refund.update", "customer_refund", "修改客户退款"),
    action("customer_sensitive.reveal", "customer_sensitive", "查看客户敏感资料"),
    action("delivery.create", "delivery", "创建实物发货单"),
    action("delivery.post", "delivery", "过账实物发货单"),
    action("delivery.update", "delivery", "修改实物发货单"),
    action("demo_master_data.restore", "customer", "恢复演示客户"),
    action("demo_master_data.restore", "party", "恢复演示主体"),
    action("demo_master_data.restore", "product_brand", "恢复演示品牌"),
    action("demo_master_data.restore", "product_category", "恢复演示商品分类"),
    action("demo_master_data.restore", "supplier", "恢复演示供应商"),
    action("demo_master_data.restore", "unit_of_measure", "恢复演示计量单位"),
    action("demo_master_data.restore", "warehouse", "恢复演示仓库"),
    action("document_attachment.create", "document_attachment", "创建单据附件"),
    action("document_participant.create", "document_participant", "登记单据参与人"),
    action("document_relation.create", "document_relation", "关联业务单据"),
    action("electronic_delivery.confirm", "electronic_delivery", "确认电子交付单"),
    action("electronic_delivery.create", "electronic_delivery", "创建电子交付单"),
    action("external_identity_map.create", "external_identity_map", "创建外部身份映射"),
    action("file_asset.destroy", "file_asset", "销毁文件资产"),
    action("file_asset.preview", "file_asset", "预览文件资产"),
    action("file_asset.register", "file_asset", "登记文件资产"),
    action("file_asset.scan", "file_asset", "登记文件安全扫描结果"),
    action("finance_responsibility_rule.create", "finance_responsibility_rule", "创建财务职责规则"),
    action("finance_responsibility_rule.update", "finance_responsibility_rule", "修改财务职责规则"),
    action("inbox_message.failed", "inbox_message", "登记处理失败外部消息"),
    action("inbox_message.processed", "inbox_message", "登记处理完成外部消息"),
    action("inbox_message.register", "inbox_message", "登记外部消息"),
    action("integration.direct_reconciliation", "reconciliation_difference", "决定对账差异"),
    action("integration.task_action", "work_item", "处理集成异常任务"),
    action("integration.task_completion", "work_item", "完成集成异常任务"),
    action("integration_error_task.create", "integration_error_task", "创建集成异常任务"),
    action("integration_error_task.work_item.create", "work_item", "创建集成异常待办"),
    action("invoice.commit", "invoice", "登记并过账销项发票"),
    action("invoice.create", "invoice", "登记发票"),
    action("invoice.post", "invoice", "登记并过账销项发票"),
    action("invoice.red_issue", "invoice", "开具红字发票"),
    action("legacy_import_batch.apply", "legacy_import_batch", "应用历史导入批次"),
    action("legacy_import_batch.create", "legacy_import_batch", "创建历史导入批次"),
    action("legacy_import_batch.execute", "legacy_import_batch", "执行历史导入命令"),
    action("legacy_import_confirmation.complete", "legacy_import_confirmation", "完成导入业务确认"),
    action("legacy_import_confirmation.create", "legacy_import_confirmation", "创建导入业务确认"),
    action("party.create", "party", "创建往来主体"),
    action("party.delete", "party", "删除往来主体"),
    action("party.update", "party", "修改往来主体"),
    action("party_address.create", "party_address", "创建主体地址"),
    action("party_address.update", "party_address", "修改主体地址"),
    action("party_bank_account.create", "party_bank_account", "创建主体银行资料"),
    action("party_bank_account.reveal_for_payment", "party_bank_account", "查看付款账户号码"),
    action("party_bank_account.update", "party_bank_account", "修改主体银行资料"),
    action("party_contact.create", "party_contact", "创建主体联系人"),
    action("party_contact.update", "party_contact", "修改主体联系人"),
    action("party_tax_profile.create", "party_tax_profile", "创建主体税务资料"),
    action("party_tax_profile.update", "party_tax_profile", "修改主体税务资料"),
    action("payable_account.create", "payable_account", "创建应付账户"),
    action("payment_reversal.cancel_approval", "payment_reversal", "撤回审批付款冲正单"),
    action("payment_reversal.create", "payment_reversal", "创建付款冲正单"),
    action("payment_reversal.post", "payment_reversal", "过账付款冲正单"),
    action("payment_reversal.submit", "payment_reversal", "提交审批付款冲正单"),
    action("payment_reversal.update", "payment_reversal", "修改付款冲正"),
    action("procurement_responsibility_rule.create", "procurement_responsibility_rule", "创建采购职责规则"),
    action("procurement_responsibility_rule.update", "procurement_responsibility_rule", "修改采购职责规则"),
    action("product.create", "product", "创建商品"),
    action("product.disable", "product", "停用商品"),
    action("product.handover", "product", "商品交接"),
    action("product.listing.update", "product", "变更商品上下架状态"),
    action("product.restore", "product", "恢复商品"),
    action("product.update", "product", "修改商品"),
    action("product_brand.create", "product_brand", "创建商品品牌"),
    action("product_brand.delete", "product_brand", "删除商品品牌"),
    action("product_brand.update", "product_brand", "修改商品品牌"),
    action("product_category.create", "product_category", "创建商品分类"),
    action("product_category.delete", "product_category", "删除商品分类"),
    action("product_category.move", "product_category", "移动商品分类"),
    action("product_category.update", "product_category", "修改商品分类"),
    action("purchase_change_order.cancel_approval", "purchase_change_order", "撤回审批采购变更单"),
    action("purchase_change_order.create", "purchase_change_order", "创建采购变更单"),
    action("purchase_change_order.effect", "purchase_change_order", "生效采购变更单"),
    action("purchase_change_order.submit", "purchase_change_order", "提交审批采购变更单"),
    action("purchase_invoice_allocation.post", "purchase_invoice_allocation", "登记进项发票"),
    action("purchase_order.cancel_approval", "purchase_order", "撤回审批采购单"),
    action("purchase_order.create_from_basis", "purchase_order", "根据采购依据建立采购单"),
    action("purchase_order.create_from_sourcing", "sales_order", "执行采购选源"),
    action("purchase_order.formalize", "purchase_order", "确认生效采购单"),
    action("purchase_order.owner_reassign", "purchase_order", "转交采购履约责任"),
    action("purchase_order.submit", "purchase_order", "提交审批采购单"),
    action("purchase_order.update", "purchase_order", "修改采购单草稿"),
    action("purchase_order.void", "purchase_order", "作废采购单"),
    action("purchase_receipt.create", "purchase_receipt", "创建采购收货单"),
    action("purchase_receipt.post", "purchase_receipt", "过账采购收货单"),
    action("purchase_receipt.update", "purchase_receipt", "修改采购收货单"),
    action("purchase_return_order.create", "purchase_return_order", "创建采购退货单"),
    action("receipt_reversal.cancel_approval", "receipt_reversal", "撤回审批收款冲正单"),
    action("receipt_reversal.create", "receipt_reversal", "创建收款冲正单"),
    action("receipt_reversal.post", "receipt_reversal", "过账收款冲正单"),
    action("receipt_reversal.submit", "receipt_reversal", "提交审批收款冲正单"),
    action("receipt_reversal.update", "receipt_reversal", "修改回款冲正"),
    action("receivable_account.create", "receivable_account", "创建应收账户"),
    action("reconciliation_difference.create", "reconciliation_difference", "创建对账差异"),
    action("returns.cancel_approval", "customer_refund", "撤回客户退款审批"),
    action("returns.cancel_approval", "payment_reversal", "撤回付款冲正审批"),
    action("returns.cancel_approval", "receipt_reversal", "撤回回款冲正审批"),
    action("returns.cancel_approval", "supplier_refund", "撤回供应商退款审批"),
    action("role.create", "role", "创建角色"),
    action("role.delete", "role", "删除角色"),
    action("role.update", "role", "修改角色"),
    action("sales_change_order.cancel_approval", "sales_change_order", "撤回审批销售变更单"),
    action("sales_change_order.create", "sales_change_order", "创建销售变更单"),
    action("sales_change_order.effective", "sales_change_order", "生效销售变更单"),
    action("sales_change_order.submit", "sales_change_order", "提交审批销售变更单"),
    action("sales_change_order.update", "sales_change_order", "修改销售变更单"),
    action("sales_change_order.void", "sales_change_order", "作废销售变更单"),
    action("sales_invoice_request.approve", "sales_invoice_request", "审批通过开票申请"),
    action("sales_invoice_request.cancel", "sales_invoice_request", "撤回开票申请"),
    action("sales_invoice_request.submit", "sales_invoice_request", "提交开票申请"),
    action("sales_order.bind_contract", "sales_order", "绑定销售合同"),
    action("sales_order.cancel_approval", "sales_order", "撤回审批销售单"),
    action("sales_order.create", "sales_order", "创建销售单"),
    action("sales_order.formalize", "sales_order", "确认生效销售单"),
    action("sales_order.handover", "sales_order", "交接销售责任"),
    action("sales_order.save_draft", "sales_order", "保存草稿销售单"),
    action("sales_order.submit", "sales_order_submission", "提交销售单"),
    action("sales_order.void", "sales_order", "作废销售单"),
    action("sales_return_case.create", "sales_return_case", "创建销售退货单"),
    action("service_fulfillment.confirm", "service_fulfillment", "确认服务履约"),
    action("service_fulfillment.create", "service_fulfillment", "创建服务履约单"),
    action("sku.listing.update", "sku", "变更规格上下架状态"),
    action("sku_attribute.create", "sku_attribute", "创建规格属性"),
    action("sku_attribute.delete", "sku_attribute", "删除规格属性"),
    action("sku_attribute.update", "sku_attribute", "修改规格属性"),
    action("sku_attribute_value.create", "sku_attribute_value", "创建规格属性值"),
    action("sku_attribute_value.delete", "sku_attribute_value", "删除规格属性值"),
    action("sku_attribute_value.update", "sku_attribute_value", "修改规格属性值"),
    action("source_system.create", "source_system", "创建来源系统"),
    action("source_system.update", "source_system", "修改来源系统"),
    action("stock_adjustment.cancel_approval", "stock_adjustment", "撤回审批库存调整单"),
    action("stock_adjustment.create", "stock_adjustment", "创建库存调整单"),
    action("stock_adjustment.post", "stock_adjustment", "过账库存调整单"),
    action("stock_adjustment.submit", "stock_adjustment", "提交审批库存调整单"),
    action("stock_adjustment.update", "stock_adjustment", "修改库存调整单"),
    action("supplier.delete", "supplier", "删除供应商"),
    action("supplier.handover", "supplier", "供应商交接"),
    action(
        "supplier_api_capability.confirm_requirement",
        "supplier_api_capability",
        "确认供应商接口采购需求",
    ),
    action("supplier_api_capability.update", "supplier_api_connection", "修改供应商接口能力配置"),
    action(
        "supplier_api_connection.bind_credential_reference",
        "supplier_api_connection",
        "绑定供应商接口凭据引用",
    ),
    action(
        "supplier_api_connection.bind_endpoint_reference",
        "supplier_api_connection",
        "绑定供应商接口地址引用",
    ),
    action("supplier_api_connection.catalog_sync.settle", "supplier_api_connection", "登记目录同步结果"),
    action("supplier_api_connection.create", "supplier_api_connection", "创建供应商接口连接"),
    action("supplier_api_connection.disable", "supplier_api_connection", "停用供应商接口连接"),
    action("supplier_api_connection.enable", "supplier_api_connection", "启用供应商接口连接"),
    action("supplier_api_connection.health_check.settle", "supplier_api_connection", "登记接口健康检查结果"),
    action("supplier_api_connection.run_health_check", "supplier_api_connection", "发起供应商接口健康检查"),
    action("supplier_api_connection.start_catalog_sync", "supplier_api_connection", "发起供应商目录同步"),
    action(
        "supplier_api_connection.update_business_profile",
        "supplier_api_connection",
        "修改供应商接口业务资料",
    ),
    action("supplier_capability.handover", "supplier_capability", "能力负责人交接"),
    action("supplier_fulfillment.after_sales_action", "supplier_order_action", "登记供应商售后操作"),
    action("supplier_fulfillment.handover", "supplier_fulfillment_order", "交接供应商履约跟进责任"),
    action("supplier_fulfillment.investigate", "SUPPLIER_FULFILLMENT_ORDER", "登记供应商履约调查"),
    action("supplier_fulfillment.refund_result", "supplier_refund_fact", "登记供应商退款结果"),
    action("supplier_fulfillment.reject", "supplier_fulfillment_order", "驳回供应商履约订单"),
    action("supplier_fulfillment.submit", "supplier_fulfillment_order", "提交审批供应商履约订单"),
    action("supplier_fulfillment.task_complete", "SUPPLIER_FULFILLMENT_ORDER", "完成供应商履约任务"),
    action("supplier_fulfillment.task_investigate", "SUPPLIER_FULFILLMENT_ORDER", "登记供应商履约任务调查"),
    action("supplier_fulfillment.work_item.create", "work_item", "创建供应商履约待办"),
    action("supplier_fulfillment.work_item.refresh_subject", "work_item", "修改供应商履约任务主题"),
    action("supplier_offering.availability.update", "supplier_offering", "修改供应商供给可用性"),
    action("supplier_offering.create", "supplier_offering", "创建供应商供给"),
    action("supplier_offering.handover", "supplier_offering", "供给交接"),
    action("supplier_offering.revise", "supplier_offering", "修订供应商供给"),
    action("supplier_offering.supply_exception.complete", "supplier_offering", "完成供给异常处理"),
    action("supplier_payment.bank_receipt.preview", "supplier_payment", "预览供应商付款银行回单"),
    action("supplier_payment.commit", "supplier_payment", "登记并过账供应商付款"),
    action("supplier_profile.create", "supplier_profile", "创建供应商档案"),
    action("supplier_profile.update", "supplier_profile", "修改供应商档案"),
    action("supplier_refund.cancel_approval", "supplier_refund", "撤回审批供应商退款单"),
    action("supplier_refund.create", "supplier_refund", "创建供应商退款单"),
    action("supplier_refund.post", "supplier_refund", "过账供应商退款单"),
    action("supplier_refund.submit", "supplier_refund", "提交审批供应商退款单"),
    action("supplier_refund.update", "supplier_refund", "修改供应商退款"),
    action("supplier_sensitive.reveal", "supplier_sensitive", "查看供应商敏感资料"),
    action("supplier_settlement.create", "supplier_settlement_statement", "创建供应商结算草稿"),
    action(
        "supplier_settlement.difference_decision",
        "supplier_settlement_difference",
        "登记供应商结算差异结论",
    ),
    action(
        "supplier_settlement.difference_evidence.append",
        "supplier_settlement_difference",
        "追加供应商结算差异证据",
    ),
    action("supplier_settlement.handover", "supplier_settlement_statement", "交接供应商结算责任"),
    action(
        "supplier_settlement.reassign_difference_handler",
        "supplier_settlement_statement",
        "转交供应商结算差异处理责任",
    ),
    action("supplier_settlement.refresh", "supplier_settlement_statement", "刷新供应商结算来源快照"),
    action("supplier_settlement.review_confirm", "supplier_settlement_statement", "确认供应商结算财务复核"),
    action("supplier_settlement.review_reject", "supplier_settlement_statement", "驳回供应商结算财务复核"),
    action(
        "supplier_settlement.source_evidence.record",
        "supplier_settlement_source_evidence",
        "登记供应商结算来源证据",
    ),
    action("supplier_settlement.submit_review", "supplier_settlement_statement", "提交供应商结算财务复核"),
    action("supplier_settlement.void", "supplier_settlement_statement", "作废供应商结算单"),
    action("unit_of_measure.create", "unit_of_measure", "创建计量单位"),
    action("unit_of_measure.delete", "unit_of_measure", "删除计量单位"),
    action("unit_of_measure.update", "unit_of_measure", "修改计量单位"),
    action("voucher_category.create", "voucher_category_profile", "创建卡券分类"),
    action("voucher_category.update", "voucher_category_profile", "修改卡券分类"),
    action("warehouse.create", "warehouse", "创建仓库"),
    action("warehouse.fulfillment_handlers.update", "warehouse", "修改仓库履约经办人"),
    action("warehouse.update", "warehouse", "修改仓库"),
    action("warehouse_sku_policy.create", "warehouse_sku_policy", "创建仓库商品策略"),
    action("warehouse_sku_policy.delete", "warehouse_sku_policy", "删除仓库商品策略"),
    action("warehouse_sku_policy.update", "warehouse_sku_policy", "修改仓库商品策略"),
    action("work_item.close", "work_item", "关闭异常任务"),
    action("work_item.reassign", "work_item", "转交任务责任"),
    action("workflow_action.append", "workflow_action", "登记工作流操作"),
];

/// 根据明确的动作和资源配对获取中文业务事件元数据。
///
/// # 参数
/// * `action` - 业务边界提供的稳定动作代码。
/// * `resource_type` - 目标资源的稳定类型代码。
///
/// # 返回
/// 返回已登记、已校验的动作元数据，默认不允许附加业务字段。
///
/// # 错误
/// 动作未登记、资源类型不匹配或登记元数据无效时返回校验错误。
pub fn registered_action(action: &str, resource_type: &str) -> Result<AuditAction> {
    let action = REGISTERED_ACTIONS
        .iter()
        .find(|metadata| metadata.code == action && metadata.resource_type == resource_type)
        .ok_or_else(|| Error::ValidationError("业务审计动作与资源类型未登记".to_string()))?;
    action.validate()?;
    Ok(*action)
}

/// 返回已登记的业务动作及资源配对，供注册覆盖和中文展示使用。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回静态业务动作目录；身份治理目录继续由身份领域维护。
///
/// # 错误
/// 无。
pub fn registered_actions() -> &'static [AuditAction] {
    REGISTERED_ACTIONS
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{registered_action, registered_actions};

    #[test]
    fn registered_pairs_are_unique_valid_and_use_chinese_labels() {
        let mut pairs = HashSet::new();
        for action in registered_actions() {
            assert!(pairs.insert((action.code, action.resource_type)));
            action.validate().unwrap();
            assert_eq!(action.version, 1);
            assert!(action.allowed_fields.is_empty() || action.resource_type == "supplier_portal_request");
            assert!(action.label.chars().any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)));
            assert_eq!(registered_action(action.code, action.resource_type).unwrap().label, action.label);
        }
    }

    #[test]
    fn unknown_actions_wrong_resources_and_embedded_ids_are_rejected() {
        assert!(registered_action("unknown.create", "customer").is_err());
        assert!(registered_action("customer.create", "supplier").is_err());
        assert!(registered_action("customer_receipt.post:receipt-1", "customer_receipt").is_err());
        assert!(registered_action(" customer.create", "customer").is_err());
    }

    #[test]
    fn explicit_resource_pairs_keep_business_namespace_and_target_semantics() {
        assert_eq!(registered_action("customer.create", "customer").unwrap().label, "创建客户");
        assert_eq!(registered_action("product.update", "product").unwrap().label, "修改商品");
        assert_eq!(
            registered_action("sales_order.submit", "sales_order_submission").unwrap().label,
            "提交销售单"
        );
        assert!(registered_action("sales_order.submit", "sales_order").is_err());
        assert_eq!(
            registered_action("supplier_fulfillment.task_complete", "SUPPLIER_FULFILLMENT_ORDER")
                .unwrap()
                .label,
            "完成供应商履约任务"
        );
        assert!(
            registered_action("supplier_fulfillment.task_complete", "supplier_fulfillment_order").is_err()
        );
        assert_eq!(
            registered_action("service_fulfillment.confirm", "service_fulfillment").unwrap().label,
            "确认服务履约"
        );
    }
}
