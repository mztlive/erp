import type { DefinitionNodeView, DocumentType, EditorNode } from "./types"
import { SALES_ORDER_PROCUREMENT_PURPOSE } from "./types"

let clientSeq = 0

/**
 * 生成编辑器本地节点标识，不作为请求 node_key。
 */
export const nextClientId = (): string => {
    clientSeq += 1
    return `editor-node-${clientSeq}`
}

/**
 * 判断节点是否为销售单采购确认用途。
 *
 * @param purpose 服务端用途
 */
export const isProcurementPurpose = (
    purpose: string | null | undefined,
): boolean => purpose === SALES_ORDER_PROCUREMENT_PURPOSE

/**
 * 构造空白审批节点。
 */
export const emptyEditorNode = (): EditorNode => ({
    client_id: nextClientId(),
    node_id: null,
    node_name: "",
    assignee_user_id: "",
    assignee_name: "",
    node_purpose: null,
    unsaved_purpose_slot: false,
})

/**
 * 构造 SalesOrder + EMPTY 首次编辑时未保存的采购确认槽位。
 */
export const unsavedProcurementSlot = (): EditorNode => ({
    client_id: nextClientId(),
    node_id: null,
    node_name: "采购确认",
    assignee_user_id: "",
    assignee_name: "",
    node_purpose: SALES_ORDER_PROCUREMENT_PURPOSE,
    unsaved_purpose_slot: true,
})

/**
 * 把服务端节点转为编辑器节点。
 *
 * @param node 服务端节点
 */
export const toEditorNode = (node: DefinitionNodeView): EditorNode => ({
    client_id: node.node_id,
    node_id: node.node_id,
    node_name: node.node_name,
    assignee_user_id: node.assignee_user_id,
    assignee_name: node.assignee_name_snapshot,
    node_purpose: node.node_purpose,
    unsaved_purpose_slot: false,
})

/**
 * 按单据类型播种草稿节点。
 *
 * `SalesOrder + 空节点` 必须展示未保存的采购确认槽位；其他类型不得出现该用途。
 *
 * @param documentType 固定单据类型
 * @param nodes 服务端节点
 */
export const seedDraftNodes = (
    documentType: DocumentType,
    nodes: readonly DefinitionNodeView[],
): EditorNode[] => {
    if (nodes.length === 0 && documentType === "sales_order") {
        return [unsavedProcurementSlot()]
    }
    return [...nodes]
        .sort((left, right) => left.display_order - right.display_order)
        .map(toEditorNode)
}

/**
 * 首次保存前把采购确认槽位固定到顺序第一。
 *
 * @param documentType 固定单据类型
 * @param nodes 编辑器节点
 */
export const orderNodesForSave = (
    documentType: DocumentType,
    nodes: readonly EditorNode[],
): EditorNode[] => {
    if (documentType !== "sales_order") return [...nodes]
    const slotIndex = nodes.findIndex((node) => node.unsaved_purpose_slot)
    if (slotIndex <= 0) return [...nodes]
    const next = [...nodes]
    const [slot] = next.splice(slotIndex, 1)
    if (!slot) return next
    return [slot, ...next]
}

/**
 * 判断节点是否允许删除或复制。采购确认用途不可删除、不可复制。
 *
 * @param node 编辑器节点
 */
export const canMutateNodeStructure = (node: EditorNode): boolean =>
    !isProcurementPurpose(node.node_purpose) && !node.unsaved_purpose_slot
