import { hasPermission } from "@/lib/permissions"

import type { DefinitionAllowedAction, DefinitionCatalogItem } from "./types"

/** 动作级权限。生成权限文件由 P0-C 注册，本阶段只按字符串收窄。 */
export const PROCESS_PERMISSIONS = {
    read: "approval_process:read",
    create: "approval_process:create",
    edit: "approval_process:edit",
    publish: "approval_process:publish",
    retire: "approval_process:retire",
} as const

const ACTION_PERMISSION: Record<DefinitionAllowedAction, string> = {
    CREATE_DRAFT: PROCESS_PERMISSIONS.create,
    REPLACE_NODES: PROCESS_PERMISSIONS.edit,
    PUBLISH: PROCESS_PERMISSIONS.publish,
    RETIRE: PROCESS_PERMISSIONS.retire,
}

/**
 * 判断当前账号是否具备目录读取权限。
 *
 * @param granted 已授予权限
 */
export const canReadCatalog = (
    granted: readonly string[] | undefined,
): boolean => hasPermission(granted, PROCESS_PERMISSIONS.read)

/**
 * 按服务端 allowed_actions 与前端权限共同收窄。前端权限不得替代服务端授权。
 *
 * @param action 目标动作
 * @param item 目录行
 * @param granted 已授予权限
 */
export const canPerformCatalogAction = (
    action: DefinitionAllowedAction,
    item: DefinitionCatalogItem,
    granted: readonly string[] | undefined,
): boolean => {
    if (item.approval_requirement === "NO_APPROVAL") return false
    if (!item.allowed_actions.includes(action)) return false
    return hasPermission(granted, ACTION_PERMISSION[action])
}

/**
 * 判断目录行是否应显示任何写入口。
 *
 * @param item 目录行
 * @param granted 已授予权限
 */
export const hasAnyWriteAction = (
    item: DefinitionCatalogItem,
    granted: readonly string[] | undefined,
): boolean =>
    item.allowed_actions.some((action) =>
        canPerformCatalogAction(action, item, granted),
    )
