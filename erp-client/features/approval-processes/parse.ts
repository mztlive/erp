import type {
    DefinitionAllowedAction,
    DefinitionCatalogItem,
    DefinitionDetailView,
    DefinitionNodeView,
    DefinitionStatus,
    DefinitionVersionItem,
    DocumentType,
    EligibleAssignee,
} from "./types"
import {
    APPROVAL_REQUIREMENTS,
    CONFIGURATION_STATUSES,
    DEFINITION_ALLOWED_ACTIONS,
    DEFINITION_STATUSES,
    DOCUMENT_TYPES,
} from "./types"

const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === "object" && value !== null && !Array.isArray(value)

const asString = (value: unknown): string | null => {
    if (typeof value === "string") return value
    if (typeof value === "number" && Number.isFinite(value))
        return String(value)
    return null
}

const asOptionalString = (value: unknown): string | null => {
    if (value == null) return null
    return asString(value)
}

const asNumber = (value: unknown): number | null => {
    if (typeof value === "number" && Number.isFinite(value)) return value
    if (typeof value === "string" && value.trim().length > 0) {
        const parsed = Number(value)
        return Number.isFinite(parsed) ? parsed : null
    }
    return null
}

const includesOf = <T extends string>(
    values: readonly T[],
    value: unknown,
): value is T => typeof value === "string" && values.includes(value as T)

/**
 * 判断值是否为合同固定单据类型。
 *
 * @param value 未知输入
 */
export const isDocumentType = (value: unknown): value is DocumentType =>
    includesOf(DOCUMENT_TYPES, value)

/**
 * 把服务端版本数字或字符串规范为字符串。
 *
 * @param value 版本字段
 */
export const asVersionString = (value: unknown): string | null =>
    asOptionalString(value)

/**
 * 解析目录行。未知单据类型或枚举时返回 null。
 *
 * @param value 单行 JSON
 */
export const parseCatalogItem = (
    value: unknown,
): DefinitionCatalogItem | null => {
    if (!isRecord(value) || !isDocumentType(value.document_type)) return null
    if (!includesOf(APPROVAL_REQUIREMENTS, value.approval_requirement)) {
        return null
    }
    if (!includesOf(CONFIGURATION_STATUSES, value.configuration_status)) {
        return null
    }
    const label = asString(value.document_type_label)
    if (!label) return null
    const actions = Array.isArray(value.allowed_actions)
        ? value.allowed_actions.filter(
              (action): action is DefinitionAllowedAction =>
                  includesOf(DEFINITION_ALLOWED_ACTIONS, action),
          )
        : []
    return {
        document_type: value.document_type,
        document_type_label: label,
        approval_requirement: value.approval_requirement,
        published_version: asVersionString(value.published_version),
        draft_version: asVersionString(value.draft_version),
        configuration_status: value.configuration_status,
        allowed_actions: actions,
    }
}

/**
 * 解析固定目录。跳过无法识别的行，不补造缺失类型。
 *
 * @param value 目录数组
 */
export const parseCatalog = (value: unknown): DefinitionCatalogItem[] => {
    if (!Array.isArray(value)) return []
    return value
        .map(parseCatalogItem)
        .filter((item): item is DefinitionCatalogItem => item != null)
}

/**
 * 解析版本列表。
 *
 * @param value 版本数组
 */
export const parseVersions = (value: unknown): DefinitionVersionItem[] => {
    if (!Array.isArray(value)) return []
    return value
        .map((item): DefinitionVersionItem | null => {
            if (!isRecord(item)) return null
            const definitionId = asString(item.definition_id)
            const version = asVersionString(item.definition_version)
            const name = asString(item.name)
            const lockVersion = asVersionString(item.definition_lock_version)
            if (
                !definitionId ||
                !version ||
                !name ||
                !lockVersion ||
                !includesOf(DEFINITION_STATUSES, item.status)
            ) {
                return null
            }
            return {
                definition_id: definitionId,
                definition_version: version,
                status: item.status,
                name,
                definition_lock_version: lockVersion,
            }
        })
        .filter((item): item is DefinitionVersionItem => item != null)
}

const parseNode = (value: unknown): DefinitionNodeView | null => {
    if (!isRecord(value)) return null
    const nodeId = asString(value.node_id)
    const nodeKey = asString(value.node_key)
    const nodeName = asString(value.node_name)
    const nodeType = asString(value.node_type)
    const displayOrder = asNumber(value.display_order)
    const assigneeUserId = asString(value.assignee_user_id)
    const assigneeName = asString(value.assignee_name_snapshot)
    if (
        !nodeId ||
        !nodeKey ||
        !nodeName ||
        !nodeType ||
        displayOrder == null ||
        !assigneeUserId ||
        assigneeName == null
    ) {
        return null
    }
    return {
        node_id: nodeId,
        node_key: nodeKey,
        node_name: nodeName,
        node_type: nodeType,
        node_purpose: asOptionalString(value.node_purpose),
        display_order: displayOrder,
        assignee_user_id: assigneeUserId,
        assignee_name_snapshot: assigneeName,
    }
}

/**
 * 解析定义详情。
 *
 * @param value 详情 JSON
 */
export const parseDefinitionDetail = (
    value: unknown,
): DefinitionDetailView | null => {
    if (!isRecord(value) || !isDocumentType(value.document_type)) return null
    const definitionId = asString(value.definition_id)
    const label = asString(value.document_type_label)
    const name = asString(value.name)
    const version = asVersionString(value.definition_version)
    const lockVersion = asVersionString(value.definition_lock_version)
    const entryNodeKey = asString(value.entry_node_key)
    const createdBy = asString(value.created_by)
    if (
        !definitionId ||
        !label ||
        !name ||
        !version ||
        !lockVersion ||
        !entryNodeKey ||
        !createdBy ||
        !includesOf(DEFINITION_STATUSES, value.status)
    ) {
        return null
    }
    const nodes = Array.isArray(value.nodes)
        ? value.nodes
              .map(parseNode)
              .filter((node): node is DefinitionNodeView => node != null)
        : []
    return {
        definition_id: definitionId,
        document_type: value.document_type,
        document_type_label: label,
        name,
        definition_version: version,
        status: value.status,
        entry_node_key: entryNodeKey,
        definition_lock_version: lockVersion,
        nodes,
        created_by: createdBy,
        published_by: asOptionalString(value.published_by),
        published_at: asNumber(value.published_at),
        retired_by: asOptionalString(value.retired_by),
        retired_at: asNumber(value.retired_at),
    }
}

/**
 * 解析定义期审批人列表。
 *
 * @param value 候选人数组
 */
export const parseEligibleAssignees = (value: unknown): EligibleAssignee[] => {
    if (!Array.isArray(value)) return []
    return value
        .map((item): EligibleAssignee | null => {
            if (!isRecord(item)) return null
            const userId = asString(item.user_id)
            const name = asString(item.name)
            if (!userId || !name) return null
            return { user_id: userId, name }
        })
        .filter((item): item is EligibleAssignee => item != null)
}

export type { DefinitionStatus }
