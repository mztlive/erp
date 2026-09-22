import { summarizePermissions } from "@/features/admin/lib/permission-catalog"
import type {
    EffectiveAccessView,
    RoleRow,
} from "@/features/access-audit/types"

export type { DataScopePreviewGroup } from "@/features/admin/lib/data-scope-preview"
export {
    formatResourceList,
    groupDataScopes,
} from "@/features/admin/lib/data-scope-preview"

export type PermissionPreview = {
    allPermissions: boolean
    count: number
    groups: readonly { name: string; count: number }[]
}

/** 操作权限摘要：列表行已有归并结果时直接用，否则按授权编码再归并一次。 */
export function permissionPreview(input: {
    previewRole?: RoleRow | null
    grants: EffectiveAccessView["moduleAndActionGrants"]
}): PermissionPreview {
    if (input.previewRole) {
        return {
            allPermissions: input.previewRole.allPermissions,
            count: input.previewRole.permissionCount,
            groups: input.previewRole.permissionGroups,
        }
    }
    const summary = summarizePermissions(
        input.grants.map((grant) => grant.capability),
    )
    return {
        allPermissions: summary.wildcard,
        count: summary.total,
        groups: summary.groups,
    }
}
