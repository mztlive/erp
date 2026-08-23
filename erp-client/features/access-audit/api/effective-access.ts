// 有效权限解释读路径：按 ROLE / USER 主体组装来源视图，不合并前端数据。

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { EffectiveAccessView } from "@/features/access-audit/types"
import type {
    BackendAdmin,
    BackendDataScope,
    BackendRole,
    BackendUserRole,
} from "./backend-types"
import { permissionLabel } from "@/features/admin/lib/permission-catalog"
import { governancePolicies, instantToIso, SCOPE_TYPE_LABEL } from "./mappers"

export async function fetchEffectiveAccess(
    subjectType: "ROLE" | "USER",
    subjectId: string,
): Promise<EffectiveAccessView | null> {
    const gp = governancePolicies()
    const permissionVersion = "pv-live"

    if (subjectType === "ROLE") {
        const roles = await apiGet<BackendRole[]>("/admin/roles")
        const role = roles.find((r) => r.id === subjectId)
        if (!role) return null
        const scopes = await apiGet<Page<BackendDataScope>>(
            "/admin/data-scopes",
            {
                page: 1,
                page_size: 50,
                subject_type: "role",
                subject_id: subjectId,
            },
        )
        const asOf = instantToIso(role.created_at) ?? ""
        return {
            subject: { type: "ROLE", id: role.id, label: role.name },
            moduleAndActionGrants: role.permissions.map((p, i) => ({
                id: `perm-${i}`,
                layer: "MODULE_ACTION" as const,
                layerLabel: "模块与动作权限",
                targetLabel: permissionLabel(p),
                capability: p,
                sourceType: "ROLE",
                sourceLabel: role.name,
            })),
            dataScopes: scopes.items.map((s) => ({
                id: s.id,
                layer: "DATA_SCOPE" as const,
                layerLabel: "数据范围",
                targetLabel: SCOPE_TYPE_LABEL[s.scope_type] ?? s.scope_type,
                capability: s.scope_targets.join("、") || "—",
                sourceType: "ROLE",
                sourceLabel: role.name,
            })),
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
    const admin = admins.find((a) => a.id === subjectId)
    if (!admin) return null

    const roles = await apiGet<BackendRole[]>("/admin/roles")
    const roleNameById = new Map(roles.map((r) => [r.id, r.name]))
    let userRoles: BackendUserRole[] = []
    try {
        userRoles = await apiGet<BackendUserRole[]>("/admin/user-roles", {
            user_id: subjectId,
        })
    } catch {
        userRoles = []
    }

    const scopes = await apiGet<Page<BackendDataScope>>("/admin/data-scopes", {
        page: 1,
        page_size: 50,
        subject_type: "user",
        subject_id: subjectId,
    })

    const asOf = instantToIso(admin.created_at) ?? ""
    return {
        subject: {
            type: "USER",
            id: admin.id,
            label: admin.name || admin.account,
        },
        moduleAndActionGrants: admin.role_ids.map((rid, i) => ({
            id: `ur-${i}`,
            layer: "MODULE_ACTION" as const,
            layerLabel: "模块与动作权限",
            targetLabel: roleNameById.get(rid) ?? rid,
            capability: "ROLE_MEMBER",
            sourceType: "USER_ROLE",
            sourceLabel: roleNameById.get(rid) ?? rid,
        })),
        dataScopes: scopes.items.map((s) => ({
            id: s.id,
            layer: "DATA_SCOPE" as const,
            layerLabel: "数据范围",
            targetLabel: SCOPE_TYPE_LABEL[s.scope_type] ?? s.scope_type,
            capability: s.scope_targets.join("、") || "—",
            sourceType: "USER",
            sourceLabel: admin.name || admin.account,
        })),
        fieldPolicies: [],
        historicalParticipantRules: [],
        deniedOrBlocked: userRoles
            .filter((ur) => ur.revoked_at != null)
            .map((ur) => ({
                id: ur.id,
                code: "REVOKED",
                message: `角色 ${ur.role_id} 已撤权`,
                layer: "MODULE_ACTION" as const,
                layerLabel: "模块与动作权限",
                sourceType: "USER_ROLE",
                sourceLabel: ur.role_id,
            })),
        permissionVersion,
        calculatedAt: asOf,
        governancePolicies: gp,
        allowedActions: ["VIEW_EFFECTIVE_ACCESS", "EMERGENCY_REVOKE_USER_ROLE"],
        actionBlockers: [],
    }
}
