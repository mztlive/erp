import type {
    CreateDefinitionDraftRequest,
    DefinitionLockRequest,
    DefinitionNodeWrite,
    DocumentType,
    DraftSource,
    EditorNode,
    ReplaceDefinitionNodesCommand,
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
 * @param idempotencyKey 本次保存的稳定幂等键
 */
export const buildReplaceNodesRequest = (
    lockVersion: string,
    nodes: readonly EditorNode[],
    idempotencyKey: string,
): ReplaceDefinitionNodesRequest => ({
    expected_definition_lock_version: lockVersion,
    nodes: buildNodeWrites(nodes),
    idempotency_key: idempotencyKey,
})

const sameNodeWrites = (
    left: readonly DefinitionNodeWrite[],
    right: readonly DefinitionNodeWrite[],
): boolean =>
    left.length === right.length &&
    left.every((node, index) => {
        const other = right[index]
        return (
            other !== undefined &&
            node.node_id === other.node_id &&
            node.node_name === other.node_name &&
            node.display_order === other.display_order &&
            node.assignee_user_id === other.assignee_user_id
        )
    })

/**
 * 为节点替换选择稳定命令。
 *
 * 相同定义、锁版本和规范化节点载荷的未决重试复用完整请求与幂等键；
 * 目标或载荷变化时才生成新键，避免同键异载荷冲突。
 *
 * @param definitionId 草稿定义主键
 * @param lockVersion 当前定义锁版本
 * @param nodes 编辑器节点
 * @param pending 上次结果未确认的命令
 * @param createIdempotencyKey 新命令键工厂
 */
export const buildStableReplaceNodesCommand = (
    definitionId: string,
    lockVersion: string,
    nodes: readonly EditorNode[],
    pending: ReplaceDefinitionNodesCommand | null,
    createIdempotencyKey: () => string,
): ReplaceDefinitionNodesCommand => {
    const nodeWrites = buildNodeWrites(nodes)
    if (
        pending?.definitionId === definitionId &&
        pending.request.expected_definition_lock_version === lockVersion &&
        sameNodeWrites(pending.request.nodes, nodeWrites)
    ) {
        return pending
    }
    return {
        definitionId,
        request: buildReplaceNodesRequest(
            lockVersion,
            nodes,
            createIdempotencyKey(),
        ),
    }
}

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
            throw new Error("提交内容包含不允许的字段，请检查后重试")
        }
    }
    if (serialized.includes("source_definition_id")) {
        throw new Error("提交内容包含不支持的字段，请刷新后重试")
    }
}

/**
 * 返回对象自有键，供测试核对请求白名单。
 *
 * @param payload 请求体
 */
export const ownKeys = (payload: object): string[] => Object.keys(payload)
