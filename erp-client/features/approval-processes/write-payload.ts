import type {
    CreateDefinitionDraftRequest,
    DefinitionLockRequest,
    DefinitionNodeWrite,
    DocumentType,
    DraftSource,
    EditorNode,
    ReplaceDefinitionNodesRequest,
} from "./types"

const FORBIDDEN_NODE_KEYS = [
    "node_key",
    "node_type",
    "node_purpose",
    "transitions",
    "role",
    "handler",
    "action",
    "assignment_mode",
    "resolver",
    "candidate_pool",
    "terminal",
] as const

/**
 * 把编辑器节点规范成从 1 连续递增的写请求节点。
 *
 * 新增节点不提交 node_id / node_key；已有节点只回传服务端 node_id。
 * 用途、连线、角色池、处理器不得进入请求。
 *
 * @param nodes 编辑器节点（已是目标顺序）
 */
export const buildNodeWrites = (
    nodes: readonly EditorNode[],
): DefinitionNodeWrite[] =>
    nodes.map((node, index) => {
        const write: DefinitionNodeWrite = {
            node_name: node.node_name.trim(),
            display_order: index + 1,
            assignee_user_id: node.assignee_user_id,
        }
        if (node.node_id) write.node_id = node.node_id
        return write
    })

/**
 * 构造整组替换节点请求体。锁版本保持字符串。
 *
 * @param lockVersion 当前定义锁版本
 * @param nodes 编辑器节点
 */
export const buildReplaceNodesRequest = (
    lockVersion: string,
    nodes: readonly EditorNode[],
): ReplaceDefinitionNodesRequest => ({
    expected_definition_lock_version: lockVersion,
    nodes: buildNodeWrites(nodes),
})

/**
 * 构造创建草稿请求。调用方必须显式给出 draft_source，不得默认。
 *
 * @param documentType 固定单据类型
 * @param name 流程名称
 * @param draftSource 空白或复制当前已发布版本
 * @param idempotencyKey 新幂等键
 */
export const buildCreateDraftRequest = (
    documentType: DocumentType,
    name: string,
    draftSource: DraftSource,
    idempotencyKey: string,
): CreateDefinitionDraftRequest => ({
    document_type: documentType,
    name: name.trim(),
    draft_source: draftSource,
    idempotency_key: idempotencyKey,
})

/**
 * 构造发布或退役请求。必须携带锁版本和新幂等键。
 *
 * @param lockVersion 当前定义锁版本
 * @param idempotencyKey 新幂等键
 */
export const buildLockRequest = (
    lockVersion: string,
    idempotencyKey: string,
): DefinitionLockRequest => ({
    expected_definition_lock_version: lockVersion,
    idempotency_key: idempotencyKey,
})

/**
 * 断言写请求节点不含禁止字段。测试与提交前自检共用。
 *
 * @param payload 将要提交的对象
 */
export const assertWritePayloadSafe = (payload: unknown): void => {
    const serialized = JSON.stringify(payload)
    for (const key of FORBIDDEN_NODE_KEYS) {
        if (serialized.includes(`"${key}"`)) {
            throw new Error("写请求包含不允许提交的字段")
        }
    }
    if (serialized.includes("source_definition_id")) {
        throw new Error("写请求不得提交源定义")
    }
}

/**
 * 返回对象自有键，供测试核对请求白名单。
 *
 * @param payload 请求体
 */
export const ownKeys = (payload: object): string[] => Object.keys(payload)
