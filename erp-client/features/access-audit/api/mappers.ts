// 后端 DTO → 客户端契约行的纯映射函数，读写路径共用。

import { summarizePermissions } from "@/features/admin/lib/permission-catalog"
import {
    auditActionLabel,
    auditObjectTypeLabel,
} from "@/features/access-audit/lib/audit-labels"
import type {
    AccessGovernancePolicyView,
    AuditEventRow,
    RoleRow,
    ScopeRow,
    UserRow,
} from "@/features/access-audit/types"
import type {
    BackendAdmin,
    BackendAuditEvent,
    BackendDataScope,
    BackendRole,
} from "./backend-types"

function instantToIso(secs: number | null | undefined): string | undefined {
    if (secs == null || !Number.isFinite(secs)) return undefined
    return new Date(secs * 1000).toISOString()
}

function matchText(hay: string, q?: string) {
    if (!q?.trim()) return true
    return hay.toLowerCase().includes(q.trim().toLowerCase())
}

function governancePolicies(): AccessGovernancePolicyView {
    // 后端无 governance policy 资源：fail-closed 默认（诚实缺口，不伪造 CONFIGURED）
    return {
        userRoleTimePolicy: {
            state: "MISSING",
            allowedActions: ["EMERGENCY_REVOKE_USER_ROLE"],
            blockerCode: "USER_ROLE_TIME_POLICY_MISSING",
        },
        fieldPolicyGranularity: {
            state: "MISSING",
            editable: false,
            blockerCode: "FIELD_POLICY_GRANULARITY_MISSING",
        },
        auditAccessPolicy: {
            state: "MISSING",
            fallbackFrom:
                instantToIso(Math.floor(Date.now() / 1000) - 7200) ?? "",
            fallbackTo: instantToIso(Math.floor(Date.now() / 1000)) ?? "",
            configurationExportAllowed: false,
            auditExportAllowed: false,
            blockerCode: "AUDIT_ACCESS_POLICY_MISSING",
        },
    }
}

const SCOPE_TYPE_LABEL: Record<BackendDataScope["scope_type"], string> = {
    company: "公司级",
    organization: "组织",
    team: "团队",
    self_owned: "本人负责",
    collaborative: "协作参与",
}

function mapAuditResult(
    r: BackendAuditEvent["result"],
): Pick<AuditEventRow, "result" | "resultLabel" | "resultTone"> {
    switch (r) {
        case "SUCCESS":
            return {
                result: "SUCCESS",
                resultLabel: "成功",
                resultTone: "success",
            }
        case "DENIED":
            return {
                result: "DENIED",
                resultLabel: "拒绝",
                resultTone: "destructive",
            }
        case "FAILED":
            return {
                result: "FAILED",
                resultLabel: "失败",
                resultTone: "warning",
            }
        case "UNKNOWN":
            return {
                result: "UNKNOWN",
                resultLabel: "未知",
                resultTone: "neutral",
            }
        default:
            return { result: "UNKNOWN", resultLabel: r, resultTone: "neutral" }
    }
}

/**
 * 角色行。
 *
 * 权限列不再打印权限编码：按模块归并成「共 N 项 · 模块 n · 模块 n」，
 * 编码留给详情面板。数据范围由 data-scopes 结果按主体 ID 关联。
 *
 * @param boundAccountCount 绑定该角色的账号数
 * @param dataScopeSummary 该角色的数据范围摘要；无记录时为「—」
 */
function toRoleRow(
    role: BackendRole,
    permissionVersion: string,
    boundAccountCount = 0,
    dataScopeSummary = "—",
): RoleRow {
    const summary = summarizePermissions(role.permissions)
    const top = summary.groups.slice(0, 3)
    const summaryText = summary.wildcard
        ? "全部权限"
        : summary.total === 0
          ? "无权限条目"
          : [
                `共 ${summary.total} 项`,
                ...top.map((group) => `${group.name} ${group.count}`),
            ].join(" · ")
    return {
        id: role.id,
        roleCode: role.id,
        name: role.name,
        status: "enabled",
        statusLabel: "启用",
        statusTone: "success",
        permissionSummary: summaryText,
        permissionCount: summary.total,
        permissionGroups: summary.groups,
        allPermissions: summary.wildcard,
        boundAccountCount,
        dataScopeSummary,
        fieldPolicySummary: "—",
        riskFlags: [],
        permissionVersion,
        organizationLabel: "—",
    }
}

/**
 * 账号行。
 *
 * @param dataScopeSummary 该账号（含其角色）的数据范围摘要；无记录时为「—」
 */
function toUserRow(
    admin: BackendAdmin,
    roleNameById: Map<string, string>,
    permissionVersion: string,
    dataScopeSummary = "—",
): UserRow {
    const activeRoles =
        admin.role_ids
            .map((id) => roleNameById.get(id) ?? id)
            .filter(Boolean)
            .join("、") || "—"
    return {
        id: admin.id,
        userId: admin.id,
        displayName: admin.name || admin.account,
        accountName: admin.account,
        roleIds: admin.role_ids,
        accountStatus: "enabled",
        statusLabel: "启用",
        statusTone: "success",
        activeRoles,
        dataScopeSummary,
        riskFlags: [],
        permissionVersion,
        organizationLabel: "—",
        roleAssignmentId: admin.role_ids[0],
    }
}

function toScopeRow(
    scope: BackendDataScope,
    labelById: Map<string, string>,
    permissionVersion: string,
): ScopeRow {
    return {
        id: scope.id,
        subjectType: scope.subject_type === "role" ? "ROLE" : "USER",
        subjectId: scope.subject_id,
        subjectLabel: labelById.get(scope.subject_id) ?? scope.subject_id,
        scopeType: scope.scope_type.toUpperCase(),
        scopeTypeLabel: SCOPE_TYPE_LABEL[scope.scope_type] ?? scope.scope_type,
        scopeTargets:
            scope.scope_targets.length > 0
                ? scope.scope_targets.join("、")
                : "—",
        permissionVersion,
        riskFlags: [],
    }
}

function toAuditRow(e: BackendAuditEvent): AuditEventRow {
    const st = mapAuditResult(e.result)
    const changed = e.changed_field_names ?? []
    return {
        auditEventId: e.id,
        recordedAt: instantToIso(e.created_at) ?? "",
        actorId: e.actor_id,
        actorLabel: e.actor_label,
        actorRole: e.actor_role,
        actionType: e.action_type,
        actionLabel: auditActionLabel(e.action_type),
        objectType: e.object_type,
        objectTypeLabel: auditObjectTypeLabel(e.object_type),
        objectId: e.object_id ?? "",
        objectLabel: e.object_label ?? e.object_id ?? "—",
        requestId: e.request_id ?? "",
        traceId: e.trace_id ?? "",
        ...st,
        changedFieldNames: changed,
        changedFieldDisplay:
            changed.length > 0
                ? changed.map((f) => `${f} · 已变更`).join("；")
                : "—",
        safeDigest: e.safe_digest ?? undefined,
    }
}

export {
    governancePolicies,
    instantToIso,
    matchText,
    SCOPE_TYPE_LABEL,
    toAuditRow,
    toRoleRow,
    toScopeRow,
    toUserRow,
}
