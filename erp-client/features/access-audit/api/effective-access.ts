// 有效权限解释读路径：按 ROLE / USER 主体组装来源视图，不合并前端数据。

import { apiGet } from "@/lib/api"
import { fetchCompleteList } from "@/lib/collect-pages"
import type { EffectiveAccessView } from "@/features/access-audit/types"
import type {
    BackendAdmin,
    BackendDataScope,
    BackendRole,
    BackendUserRole,
} from "./backend-types"
import { permissionLabel } from "@/features/admin/lib/permission-catalog"
import { governancePolicies, instantToIso, SCOPE_TYPE_LABEL } from "./mappers"

function toDataScopeGrant(
    scope: BackendDataScope,
    sourceType: "ROLE" | "USER",
    sourceLabel: string,
): EffectiveAccessView["dataScopes"][number] {
    return {
        id: scope.id,
        layer: "DATA_SCOPE",
        layerLabel: "数据范围",
        targetLabel: SCOPE_TYPE_LABEL[scope.scope_type] ?? scope.scope_type,
        capability: scope.resource ?? "",
        sourceType,
        sourceLabel,
        resource: scope.resource,
        actions: scope.actions,
        scopeType: scope.scope_type,
        scopeTargets: scope.scope_targets,
    }
}

async function fetchSubjectDataScopes(
    subjectType: "role" | "user",
    subjectId: string,
): Promise<BackendDataScope[]> {
    const page = await fetchCompleteList<BackendDataScope>(
        "/admin/data-scopes",
        {
            subject_type: subjectType,
            subject_id: subjectId,
        },
    )
    return page.items
}

export async function fetchEffectiveAccess(
    subjectType: "ROLE" | "USER",
    subjectId: string,
): Promise<EffectiveAccessView | null> {
    const gp = governancePolicies()
    const permissionVersion = "pv-live"

    if (subjectType === "ROLE") {
        const roles = await apiGet<BackendRole[]>("/admin/roles")
        const role = roles.find((candidate) => candidate.id === subjectId)
        if (!role) return null
        const scopes = await fetchSubjectDataScopes("role", subjectId)
        const asOf = instantToIso(role.created_at) ?? ""
        return {
            subject: { type: "ROLE", id: role.id, label: role.name },
            moduleAndActionGrants: role.permissions.map((code, index) => ({
                id: `perm-${index}`,
                layer: "MODULE_ACTION" as const,
                layerLabel: "模块与动作权限",
                targetLabel: permissionLabel(code),
                capability: code,
                sourceType: "ROLE",
                sourceLabel: role.name,
            })),
            dataScopes: scopes.map((scope) =>
                toDataScopeGrant(scope, "ROLE", role.name),
            ),
            fieldPolicies: [],
            historicalParticipantRules: [],
            deniedOrBlocked: [],
            permissionVersion,
            calculatedAt: asOf,
            governancePolicies: gp,
            allowedActions: ["VIEW_EFFECTIVE_ACCESS"],
            actionBlockers: [],
        }
    }

    const admins = await apiGet<BackendAdmin[]>("/admin/admins")
    const admin = admins.find((candidate) => candidate.id === subjectId)
    if (!admin) return null

    const roles = await apiGet<BackendRole[]>("/admin/roles")
    const roleNameById = new Map(roles.map((role) => [role.id, role.name]))
    let userRoles: BackendUserRole[] = []
    try {
        userRoles = await apiGet<BackendUserRole[]>("/admin/user-roles", {
            user_id: subjectId,
        })
    } catch {
        userRoles = []
    }

    const scopes = await fetchSubjectDataScopes("user", subjectId)
    const asOf = instantToIso(admin.created_at) ?? ""
    const sourceLabel = admin.name || admin.account
    return {
        subject: {
            type: "USER",
            id: admin.id,
            label: sourceLabel,
        },
        moduleAndActionGrants: admin.role_ids.map((roleId, index) => ({
            id: `ur-${index}`,
            layer: "MODULE_ACTION" as const,
            layerLabel: "模块与动作权限",
            targetLabel: roleNameById.get(roleId) ?? roleId,
            capability: "ROLE_MEMBER",
            sourceType: "USER_ROLE",
            sourceLabel: roleNameById.get(roleId) ?? roleId,
        })),
        dataScopes: scopes.map((scope) =>
            toDataScopeGrant(scope, "USER", sourceLabel),
        ),
        fieldPolicies: [],
        historicalParticipantRules: [],
        deniedOrBlocked: userRoles
            .filter((assignment) => assignment.revoked_at != null)
            .map((assignment) => ({
                id: assignment.id,
                code: "REVOKED",
                message: `角色 ${assignment.role_id} 已撤权`,
                layer: "MODULE_ACTION" as const,
                layerLabel: "模块与动作权限",
                sourceType: "USER_ROLE",
                sourceLabel: assignment.role_id,
            })),
        permissionVersion,
        calculatedAt: asOf,
        governancePolicies: gp,
        allowedActions: ["VIEW_EFFECTIVE_ACCESS", "EMERGENCY_REVOKE_USER_ROLE"],
        actionBlockers: [],
    }
}
