// 有效权限解释读路径：按 ROLE / USER 主体组装来源视图，不合并前端数据。

import { apiGet } from "@/lib/api"
import { fetchPersonScopes } from "@/features/admin/api/person-data-scopes"
import { scopeViewRows } from "./person-scopes"
import type { EffectiveAccessView } from "@/features/access-audit/types"
import type {
    BackendAdmin,
    BackendDataScope,
    BackendRole,
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
        targetDimension: scope.target_dimension,
        targetMode: scope.target_mode,
        includeDescendants: scope.include_descendants,
        enabled: scope.enabled,
    }
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
        const scopes: BackendDataScope[] = []
        const asOf = instantToIso(role.created_at) ?? ""
        return {
            subject: { type: "ROLE", id: role.id, label: role.name },
            moduleAndActionGrants: role.permissions.map((code, index) => ({
                id: `perm-${index}`,
                layer: "MODULE_ACTION" as const,
                layerLabel: "功能操作权限",
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

    const scopes = await fetchPersonScopes(subjectId)
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
            layerLabel: "功能操作权限",
            targetLabel: roleNameById.get(roleId) ?? roleId,
            capability: "ROLE_MEMBER",
            sourceType: "USER_ROLE",
            sourceLabel: roleNameById.get(roleId) ?? roleId,
        })),
        dataScopes: [
            ...scopeViewRows(subjectId, scopes).map((scope) => ({
                id: scope.id,
                layer: "DATA_SCOPE" as const,
                layerLabel: "数据可见范围",
                targetLabel: scope.summary ?? "无数据访问范围",
                capability: scope.resource ?? "",
                sourceType: "USER",
                sourceLabel,
                resource: scope.resource,
                actions: scope.actions,
                enabled: scope.enabled,
            })),
            ...scopes.businesses.flatMap((business) => {
                const actions = business.actions.filter(
                    (action) => !business.configurable_actions.includes(action),
                )
                return actions.length
                    ? [
                          {
                              id: `policy:${business.resource}`,
                              layer: "DATA_SCOPE" as const,
                              layerLabel: "业务访问规则",
                              targetLabel: business.policy_description,
                              capability: business.resource,
                              sourceType: "AUTHORIZATION_POLICY",
                              sourceLabel,
                              resource: business.resource,
                              actions,
                          },
                      ]
                    : []
            }),
        ],
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
