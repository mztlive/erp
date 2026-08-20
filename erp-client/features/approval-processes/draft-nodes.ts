import type { DefinitionNodeView, DocumentType, EditorNode } from "./types"

let clientSeq = 0

/**
 * 生成编辑器本地节点标识，不作为请求 node_key。
 */
export const nextClientId = (): string => {
    clientSeq += 1
    return `editor-node-${clientSeq}`
}

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
 * 销售单空白草稿的预置节点。只是默认名称，可删除、可改名，不锁定用途。
 */
export const defaultSalesOrderNode = (): EditorNode => ({
    ...emptyEditorNode(),
    node_name: "采购确认",
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
 * 销售单空草稿预置一个名为「采购确认」的普通节点，允许删除；其它类型空草稿为零节点。
 *
 * @param documentType 固定单据类型
 * @param nodes 服务端节点
 */
export const seedDraftNodes = (
    documentType: DocumentType,
    nodes: readonly DefinitionNodeView[],
): EditorNode[] => {
    if (nodes.length === 0 && documentType === "sales_order") {
        return [defaultSalesOrderNode()]
    }
    return [...nodes]
        .sort((left, right) => left.display_order - right.display_order)
        .map(toEditorNode)
}

/**
 * 保存前按当前编辑顺序输出节点。销售单不再强制某一节点排第一。
 *
 * @param documentType 固定单据类型
 * @param nodes 编辑器节点
 */
export const orderNodesForSave = (
    _documentType: DocumentType,
    nodes: readonly EditorNode[],
): EditorNode[] => [...nodes]

/**
 * 判断节点是否允许删除或调整结构。全部节点均可删除。
 *
 * @param _node 编辑器节点
 */
export const canMutateNodeStructure = (_node: EditorNode): boolean => true
