import type { StatusTone } from "@/components/ui/status-badge"
import { versionText } from "@/lib/ui-text"

import type {
    ApprovalRequirement,
    ConfigurationStatus,
    DefinitionAllowedAction,
    DefinitionStatus,
    DocumentType,
    DraftSource,
} from "./types"
import { SALES_ORDER_PROCUREMENT_PURPOSE } from "./types"

/** 合同 §4.3 单据类型中文名；目录优先使用服务端 document_type_label。 */
export const DOCUMENT_TYPE_LABEL: Record<DocumentType, string> = {
    sales_order: "销售单（实物及服务）",
    voucher_sales_order: "卡券销售单",
    sales_change_order: "销售变更单",
    purchase_order: "采购单",
    purchase_change_order: "采购变更单",
    stock_adjustment: "库存调整单",
    customer_receipt: "客户回款单",
    supplier_payment: "供应商付款单",
    customer_refund: "客户退款单",
    supplier_refund: "供应商退款单",
    receipt_reversal: "回款冲正单",
    payment_reversal: "付款冲正单",
    purchase_receipt: "采购收货单",
    delivery: "仓发单",
    electronic_delivery: "电子交付单",
    service_fulfillment: "服务履约单",
    customer_acceptance: "客户验收单",
    invoice: "发票",
    sales_return_case: "销售退货单",
    purchase_return_order: "采购退货单",
}

/** 审批政策中文。 */
export const APPROVAL_REQUIREMENT_LABEL: Record<ApprovalRequirement, string> = {
    NO_APPROVAL: "无需审批",
    PROCESS_REQUIRED: "必须审批",
}

/** 配置状态中文。不得把配置缺失显示为无需审批。 */
export const CONFIGURATION_STATUS_LABEL: Record<ConfigurationStatus, string> = {
    NOT_APPLICABLE: "无需审批 / 不适用",
    MISSING_CONFIGURATION: "配置缺失",
    DRAFT: "有草稿",
    PUBLISHED: "已发布",
}

/** 定义状态中文。 */
export const DEFINITION_STATUS_LABEL: Record<DefinitionStatus, string> = {
    DRAFT: "草稿",
    PUBLISHED: "已发布",
    RETIRED: "已退役",
}

/** 草稿来源中文。 */
export const DRAFT_SOURCE_LABEL: Record<DraftSource, string> = {
    EMPTY: "空白流程",
    CURRENT_PUBLISHED: "复制当前已发布版本",
}

/** 目录动作按钮文案。 */
export const ALLOWED_ACTION_LABEL: Record<DefinitionAllowedAction, string> = {
    CREATE_DRAFT: "新建草稿",
    REPLACE_NODES: "继续编辑",
    PUBLISH: "发布",
    RETIRE: "退役",
}

/**
 * 返回单据类型用户可见名称。
 *
 * @param documentType 固定单据类型
 * @param serverLabel 服务端目录/详情标签
 */
export const documentTypeLabel = (
    documentType: DocumentType,
    serverLabel?: string,
): string => {
    const trimmed = serverLabel?.trim()
    if (trimmed) return trimmed
    return DOCUMENT_TYPE_LABEL[documentType]
}

/**
 * 返回审批政策用户可见文案。
 *
 * @param requirement 审批政策
 */
export const approvalRequirementLabel = (
    requirement: ApprovalRequirement,
): string => APPROVAL_REQUIREMENT_LABEL[requirement]

/**
 * 返回配置状态用户可见文案。
 *
 * `PROCESS_REQUIRED + MISSING_CONFIGURATION` 必须显示阻断，不得写成无需审批。
 *
 * @param status 配置状态
 * @param requirement 审批政策
 */
export const configurationStatusLabel = (
    status: ConfigurationStatus,
    requirement: ApprovalRequirement,
): string => {
    if (
        requirement === "PROCESS_REQUIRED" &&
        status === "MISSING_CONFIGURATION"
    ) {
        return "配置缺失"
    }
    if (requirement === "NO_APPROVAL") {
        return "无需审批 / 不适用"
    }
    return CONFIGURATION_STATUS_LABEL[status]
}

/**
 * 返回配置状态徽章色相。配置缺失必须是阻断色，不得用中性色淡化。
 */
export const configurationStatusTone = (
    status: ConfigurationStatus,
    requirement: ApprovalRequirement,
): StatusTone => {
    if (
        requirement === "PROCESS_REQUIRED" &&
        status === "MISSING_CONFIGURATION"
    ) {
        return "destructive"
    }
    if (status === "PUBLISHED") return "success"
    if (status === "DRAFT") return "warning"
    return "neutral"
}

/**
 * 返回定义状态徽章色相。
 */
export const definitionStatusTone = (status: DefinitionStatus): StatusTone => {
    if (status === "PUBLISHED") return "success"
    if (status === "DRAFT") return "warning"
    return "void"
}

/**
 * 返回定义状态用户可见文案；未知值不回显原值。
 *
 * @param status 定义状态
 */
export const definitionStatusLabel = (status: DefinitionStatus): string =>
    DEFINITION_STATUS_LABEL[status]

/**
 * 返回业务版本展示，空值显示「—」。
 *
 * @param version 字符串版本
 */
export const versionLabel = (version: string | null | undefined): string => {
    const trimmed = version?.trim()
    if (!trimmed) return "—"
    return `${versionText.version} ${trimmed}`
}

/**
 * 返回节点用途只读文案。采购确认不得显示内部常量或旧任务类型名。
 *
 * @param purpose 服务端用途
 */
export const nodePurposeLabel = (
    purpose: string | null | undefined,
): string => {
    if (!purpose) return "普通审批节点"
    if (purpose === SALES_ORDER_PROCUREMENT_PURPOSE) return "采购确认"
    return "指定用途节点"
}

/**
 * 返回审批人资格展示。服务端已过滤，页面只说明静态资格结果。
 *
 * @param name 审批人显示名
 */
export const assigneeEligibilityLabel = (name: string): string =>
    `${name} · 账号有效，符合定义期资格`

/** 固定驳回语义说明。 */
export const REJECT_RESTART_COPY =
    "任一层驳回后，将从第一位审批人开始下一轮审批。"

/**
 * 把线性审批人显示名拼成发布预览路径。
 *
 * @param names 按顺序的审批人显示名
 */
export const publishPathPreview = (names: readonly string[]): string =>
    names.filter((name) => name.trim().length > 0).join(" → ")
