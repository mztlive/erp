import type {
    DefinitionAllowedAction,
    DefinitionCatalogItem,
    DefinitionDetailView,
    DefinitionVersionItem,
    DocumentType,
} from "./types"
import {
    APPROVAL_REQUIREMENTS,
    CONFIGURATION_STATUSES,
    DEFINITION_ALLOWED_ACTIONS,
    DEFINITION_STATUSES,
    DOCUMENT_TYPES,
} from "./types"

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
 * 解析目录。未知单据类型或枚举时跳过该行，不补造缺失类型。
 *
 * @param items 目录数组
 */
export const parseCatalog = (
    items: readonly DefinitionCatalogItem[],
): DefinitionCatalogItem[] =>
    items.flatMap((item) => {
        if (
            !isDocumentType(item.document_type) ||
            !includesOf(APPROVAL_REQUIREMENTS, item.approval_requirement) ||
            !includesOf(CONFIGURATION_STATUSES, item.configuration_status)
        ) {
            return []
        }
        return [
            {
                ...item,
                allowed_actions: item.allowed_actions.filter(
                    (action): action is DefinitionAllowedAction =>
                        includesOf(DEFINITION_ALLOWED_ACTIONS, action),
                ),
            },
        ]
    })

/**
 * 解析版本列表。
 *
 * @param items 版本数组
 */
export const parseVersions = (
    items: readonly DefinitionVersionItem[],
): DefinitionVersionItem[] =>
    items.filter((item) => includesOf(DEFINITION_STATUSES, item.status))

/**
 * 解析定义详情。未知单据类型或状态时返回 null。
 *
 * @param value 详情
 */
export const parseDefinitionDetail = (
    value: DefinitionDetailView,
): DefinitionDetailView | null => {
    if (
        !isDocumentType(value.document_type) ||
        !includesOf(DEFINITION_STATUSES, value.status)
    ) {
        return null
    }
    return value
}
