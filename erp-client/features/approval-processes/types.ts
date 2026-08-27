/**
 * 审批流程配置（W24）与阶段 06 HTTP DTO 对齐的线协议类型。
 *
 * 版本字段保持字符串，避免 JS 不安全整数。不得复用卡券审批 DTO，
 * 也不得建模 ProcessKind、SubjectRef、TransitionPlan 或内部 BPM 事件。
 */

/** 合同 §4.3 固定 20 个单据类型（snake_case 线协议值）。 */
export const DOCUMENT_TYPES = [
    "sales_order",
    "voucher_sales_order",
    "sales_change_order",
    "purchase_order",
    "purchase_change_order",
    "stock_adjustment",
    "customer_receipt",
    "supplier_payment",
    "customer_refund",
    "supplier_refund",
    "receipt_reversal",
    "payment_reversal",
    "purchase_receipt",
    "delivery",
    "electronic_delivery",
    "service_fulfillment",
    "customer_acceptance",
    "invoice",
    "sales_return_case",
    "purchase_return_order",
] as const

export type DocumentType = (typeof DOCUMENT_TYPES)[number]

/** 合同签署为无需审批的 9 个类型。 */
export const NO_APPROVAL_DOCUMENT_TYPES = [
    "supplier_payment",
    "purchase_receipt",
    "delivery",
    "electronic_delivery",
    "service_fulfillment",
    "customer_acceptance",
    "invoice",
    "sales_return_case",
    "purchase_return_order",
] as const satisfies readonly DocumentType[]

/** 合同签署为必须审批的 11 个类型。 */
export const PROCESS_REQUIRED_DOCUMENT_TYPES = [
    "sales_order",
    "voucher_sales_order",
    "sales_change_order",
    "purchase_order",
    "purchase_change_order",
    "stock_adjustment",
    "customer_receipt",
    "customer_refund",
    "supplier_refund",
    "receipt_reversal",
    "payment_reversal",
] as const satisfies readonly DocumentType[]

export type NoApprovalDocumentType = (typeof NO_APPROVAL_DOCUMENT_TYPES)[number]
export type ProcessRequiredDocumentType =
    (typeof PROCESS_REQUIRED_DOCUMENT_TYPES)[number]

/** 审批政策。 */
export const APPROVAL_REQUIREMENTS = [
    "NO_APPROVAL",
    "PROCESS_REQUIRED",
] as const

export type ApprovalRequirement = (typeof APPROVAL_REQUIREMENTS)[number]

/** 目录配置状态。 */
export const CONFIGURATION_STATUSES = [
    "NOT_APPLICABLE",
    "MISSING_CONFIGURATION",
    "DRAFT",
    "PUBLISHED",
] as const

export type ConfigurationStatus = (typeof CONFIGURATION_STATUSES)[number]

/** 当前用户对某类型允许的定义管理动作。 */
export const DEFINITION_ALLOWED_ACTIONS = [
    "CREATE_DRAFT",
    "REPLACE_NODES",
    "PUBLISH",
    "RETIRE",
] as const

export type DefinitionAllowedAction =
    (typeof DEFINITION_ALLOWED_ACTIONS)[number]

/** 定义生命周期状态。 */
export const DEFINITION_STATUSES = ["DRAFT", "PUBLISHED", "RETIRED"] as const

export type DefinitionStatus = (typeof DEFINITION_STATUSES)[number]

/** 草稿创建来源。 */
export const DRAFT_SOURCES = ["EMPTY", "CURRENT_PUBLISHED"] as const

export type DraftSource = (typeof DRAFT_SOURCES)[number]

/** 历史销售单采购确认用途。页面不再锁定，请求不得写入该字段。 */
export const SALES_ORDER_PROCUREMENT_PURPOSE =
    "SALES_ORDER_PROCUREMENT_CONFIRMATION"

/** 固定单据类型目录行。 */
export type DefinitionCatalogItem = {
    document_type: DocumentType
    document_type_label: string
    approval_requirement: ApprovalRequirement
    published_version: string | null
    draft_version: string | null
    configuration_status: ConfigurationStatus
    allowed_actions: DefinitionAllowedAction[]
}

/** 历史版本摘要。 */
export type DefinitionVersionItem = {
    definition_id: string
    definition_version: string
    status: DefinitionStatus
    name: string
    definition_lock_version: string
}

/** 定义节点详情。 */
export type DefinitionNodeView = {
    node_id: string
    node_key: string
    node_name: string
    node_type: string
    node_purpose: string | null
    display_order: number
    assignee_user_id: string
    assignee_name_snapshot: string
}

/** 定义详情。 */
export type DefinitionDetailView = {
    definition_id: string
    document_type: DocumentType
    document_type_label: string
    name: string
    definition_version: string
    status: DefinitionStatus
    entry_node_key: string
    definition_lock_version: string
    nodes: DefinitionNodeView[]
    created_by: string
    published_by: string | null
    published_at: number | null
    retired_by: string | null
    retired_at: number | null
}

/** 定义期可选审批人。 */
export type EligibleAssignee = {
    user_id: string
    name: string
}

/** 创建草稿请求。不得携带源定义 ID。 */
export type CreateDefinitionDraftRequest = {
    document_type: DocumentType
    name: string
    draft_source: DraftSource
    idempotency_key: string
}

/** 草稿节点写请求。只允许定位、名称、顺序和指定审批人。 */
export type DefinitionNodeWrite = {
    node_id?: string
    node_name: string
    display_order: number
    assignee_user_id: string
}

/** 整组替换草稿节点的 HTTP 请求体。 */
export type ReplaceDefinitionNodesRequest = {
    expected_definition_lock_version: string
    nodes: DefinitionNodeWrite[]
}

/** 发布或退役请求体。 */
export type DefinitionLockRequest = {
    expected_definition_lock_version: string
    idempotency_key: string
}

/** 编辑器本地节点（含未保存槽位）。 */
export type EditorNode = {
    client_id: string
    node_id: string | null
    node_name: string
    assignee_user_id: string
    assignee_name: string
    node_purpose: string | null
    unsaved_purpose_slot: boolean
}

/** 草稿编辑器表单值。 */
export type DefinitionEditorValues = {
    name: string
    nodes: EditorNode[]
}

/** 创建草稿表单值。 */
export type CreateDraftFormValues = {
    name: string
    draft_source: DraftSource | ""
}

/** 目录 URL 状态。 */
export type CatalogUrlState = {
    policy: ApprovalRequirement | "ALL"
    status: ConfigurationStatus | "HAS_DRAFT" | "ALL"
    q: string
    page: number
}

/** 详情 URL 状态。 */
export type DetailView = "current" | "draft" | "history"

export type DetailUrlState = {
    view: DetailView
    version?: string
}
