import { DOCUMENT_TYPE_LABEL } from "./labels"
import type {
    DefinitionCatalogItem,
    DefinitionDetailView,
    DefinitionNodeView,
    DefinitionVersionItem,
    DocumentType,
    EligibleAssignee,
} from "./types"
import {
    DOCUMENT_TYPES,
    NO_APPROVAL_DOCUMENT_TYPES,
    PROCESS_REQUIRED_DOCUMENT_TYPES,
    SALES_ORDER_PROCUREMENT_PURPOSE,
} from "./types"

const noApprovalSet = new Set<string>(NO_APPROVAL_DOCUMENT_TYPES)

/**
 * 构造合同 §4.3 完整 20 行目录夹具。
 *
 * @param overrides 按类型覆盖
 */
export const catalogFixture = (
    overrides: Partial<
        Record<DocumentType, Partial<DefinitionCatalogItem>>
    > = {},
): DefinitionCatalogItem[] =>
    DOCUMENT_TYPES.map((documentType) => {
        const noApproval = noApprovalSet.has(documentType)
        const base: DefinitionCatalogItem = noApproval
            ? {
                  document_type: documentType,
                  document_type_label: DOCUMENT_TYPE_LABEL[documentType],
                  approval_requirement: "NO_APPROVAL",
                  published_version: null,
                  draft_version: null,
                  configuration_status: "NOT_APPLICABLE",
                  allowed_actions: [],
              }
            : {
                  document_type: documentType,
                  document_type_label: DOCUMENT_TYPE_LABEL[documentType],
                  approval_requirement: "PROCESS_REQUIRED",
                  published_version: null,
                  draft_version: null,
                  configuration_status: "MISSING_CONFIGURATION",
                  allowed_actions: ["CREATE_DRAFT"],
              }
        return { ...base, ...overrides[documentType] }
    })

/**
 * 构造节点夹具。
 *
 * @param input 部分节点
 */
export const nodeFixture = (
    input: Partial<DefinitionNodeView> &
        Pick<DefinitionNodeView, "node_id" | "node_name">,
): DefinitionNodeView => ({
    node_key: input.node_key ?? `node_${input.node_id}`,
    node_type: "USER_APPROVAL",
    node_purpose: input.node_purpose ?? null,
    display_order: input.display_order ?? 1,
    assignee_user_id: input.assignee_user_id ?? "user-zhang",
    assignee_name_snapshot: input.assignee_name_snapshot ?? "张三",
    ...input,
})

/**
 * 构造定义详情夹具。
 *
 * @param input 部分详情
 */
export const detailFixture = (
    input: Partial<DefinitionDetailView> = {},
): DefinitionDetailView => ({
    definition_id: input.definition_id ?? "def-stock-1",
    document_type: input.document_type ?? "stock_adjustment",
    document_type_label:
        input.document_type_label ?? DOCUMENT_TYPE_LABEL.stock_adjustment,
    name: input.name ?? "库存调整审批",
    definition_version: input.definition_version ?? "1",
    status: input.status ?? "DRAFT",
    entry_node_key: input.entry_node_key ?? "n1",
    definition_lock_version: input.definition_lock_version ?? "3",
    nodes: input.nodes ?? [
        nodeFixture({
            node_id: "n1",
            node_name: "仓储复核",
            display_order: 1,
            assignee_name_snapshot: "张三",
        }),
    ],
    created_by: input.created_by ?? "admin-1",
    published_by: input.published_by ?? null,
    published_at: input.published_at ?? null,
    retired_by: input.retired_by ?? null,
    retired_at: input.retired_at ?? null,
})

/**
 * 销售单空白草稿：无服务端节点，编辑器预置可删除的默认节点。
 */
export const salesOrderEmptyDraft = (): DefinitionDetailView =>
    detailFixture({
        definition_id: "def-sales-empty",
        document_type: "sales_order",
        document_type_label: DOCUMENT_TYPE_LABEL.sales_order,
        name: "销售单审批",
        nodes: [],
    })

/**
 * 销售单已保存草稿：可含历史采购确认用途，节点均可删除。
 */
export const salesOrderSavedDraft = (): DefinitionDetailView =>
    detailFixture({
        definition_id: "def-sales-saved",
        document_type: "sales_order",
        document_type_label: DOCUMENT_TYPE_LABEL.sales_order,
        name: "销售单审批",
        nodes: [
            nodeFixture({
                node_id: "n-proc",
                node_name: "采购确认",
                node_purpose: SALES_ORDER_PROCUREMENT_PURPOSE,
                display_order: 1,
                assignee_user_id: "user-li",
                assignee_name_snapshot: "李四",
            }),
            nodeFixture({
                node_id: "n-lead",
                node_name: "销售复核",
                display_order: 2,
                assignee_user_id: "user-wang",
                assignee_name_snapshot: "王五",
            }),
        ],
    })

/**
 * 版本历史夹具。
 */
export const versionsFixture = (): DefinitionVersionItem[] => [
    {
        definition_id: "def-stock-2",
        definition_version: "2",
        status: "DRAFT",
        name: "库存调整审批",
        definition_lock_version: "1",
    },
    {
        definition_id: "def-stock-1",
        definition_version: "1",
        status: "PUBLISHED",
        name: "库存调整审批",
        definition_lock_version: "4",
    },
]

/**
 * 审批人候选夹具。
 */
export const assigneesFixture = (): EligibleAssignee[] => [
    { user_id: "user-zhang", name: "张三" },
    { user_id: "user-li", name: "李四" },
    { user_id: "user-wang", name: "王五" },
]

export { PROCESS_REQUIRED_DOCUMENT_TYPES, NO_APPROVAL_DOCUMENT_TYPES }
