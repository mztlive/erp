// 聚合列表读路径：roles + admins + data-scopes + audit-events + permissions。
// field policies 后端无资源：返回空列表并登记 gap，不造业务数据。

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
    AccessEmptyReason,
    AccessListQuery,
    AccessListView,
    FieldPolicyRow,
} from "@/features/access-audit/types"
import type {
    BackendAdmin,
    BackendAuditEvent,
    BackendDataScope,
    BackendPermission,
    BackendRole,
} from "./backend-types"
import {
    governancePolicies,
    instantToIso,
    matchText,
    toAuditRow,
    toRoleRow,
    toScopeRow,
    toUserRow,
} from "./mappers"

export async function fetchAccessList(
    query: AccessListQuery,
): Promise<AccessListView> {
    const gp = governancePolicies()
    const permissionVersion = `pv-live`

    const [roles, admins, scopesPage, auditPage, permsPage] = await Promise.all(
        [
            apiGet<BackendRole[]>("/admin/roles"),
            apiGet<BackendAdmin[]>("/admin/admins"),
            apiGet<Page<BackendDataScope>>("/admin/data-scopes", {
                page: 1,
                page_size: 100,
                subject_type:
                    query.subjectType === "ROLE"
                        ? "role"
                        : query.subjectType === "USER"
                          ? "user"
                          : undefined,
                subject_id: query.subjectId,
            }),
            apiGet<Page<BackendAuditEvent>>("/admin/audit-events", {
                page: 1,
                page_size: 50,
                actor_id: query.actorId,
                action_type: query.action,
                object_type: query.objectType,
                object_id: query.objectId,
                result: query.result as BackendAuditEvent["result"] | undefined,
            }),
            apiGet<Page<BackendPermission>>("/admin/permissions", {
                page: 1,
                page_size: 50,
            }),
        ],
    )

    const roleNameById = new Map(roles.map((r) => [r.id, r.name]))
    const labelById = new Map<string, string>([
        ...roles.map((r) => [r.id, r.name] as const),
        ...admins.map((a) => [a.id, a.name || a.account] as const),
    ])

    let roleRows = roles.map((r) => toRoleRow(r, permissionVersion))
    let userRows = admins.map((a) =>
        toUserRow(a, roleNameById, permissionVersion),
    )
    let scopeRows = scopesPage.items.map((s) =>
        toScopeRow(s, labelById, permissionVersion),
    )
    // field policies：后端无 field_policy 资源
    const fieldPolicies: FieldPolicyRow[] = []
    let auditEvents = auditPage.items.map(toAuditRow)

    if (query.status === "enabled") {
        roleRows = roleRows.filter((r) => r.status === "enabled")
        userRows = userRows.filter((u) => u.accountStatus === "enabled")
    } else if (query.status === "disabled") {
        roleRows = roleRows.filter((r) => r.status === "disabled")
        userRows = userRows.filter((u) => u.accountStatus === "disabled")
    }
    if (query.risk) {
        roleRows = roleRows.filter((r) => r.riskFlags.includes(query.risk!))
        userRows = userRows.filter((u) => u.riskFlags.includes(query.risk!))
        scopeRows = scopeRows.filter((s) => s.riskFlags.includes(query.risk!))
    }
    if (query.org) {
        roleRows = roleRows.filter((r) =>
            matchText(r.organizationLabel, query.org),
        )
        userRows = userRows.filter((u) =>
            matchText(u.organizationLabel, query.org),
        )
    }
    if (query.q) {
        roleRows = roleRows.filter((r) =>
            matchText(
                `${r.name} ${r.roleCode} ${r.permissionSummary}`,
                query.q,
            ),
        )
        userRows = userRows.filter((u) =>
            matchText(`${u.displayName} ${u.userId} ${u.activeRoles}`, query.q),
        )
        scopeRows = scopeRows.filter((s) =>
            matchText(
                `${s.subjectLabel} ${s.scopeTypeLabel} ${s.scopeTargets}`,
                query.q,
            ),
        )
        auditEvents = auditEvents.filter((e) =>
            matchText(
                `${e.actorLabel} ${e.actionLabel} ${e.objectLabel} ${e.traceId}`,
                query.q,
            ),
        )
    }
    if (query.traceId) {
        auditEvents = auditEvents.filter(
            (e) => e.traceId === query.traceId || e.requestId === query.traceId,
        )
    }
    if (query.eventId) {
        auditEvents = auditEvents.filter(
            (e) => e.auditEventId === query.eventId,
        )
    }
    if (query.from) {
        auditEvents = auditEvents.filter((e) => e.recordedAt >= query.from!)
    }
    if (query.to) {
        auditEvents = auditEvents.filter((e) => e.recordedAt <= query.to!)
    }

    const rowsForView =
        query.view === "roles"
            ? roleRows
            : query.view === "users"
              ? userRows
              : query.view === "scopes"
                ? scopeRows
                : query.view === "fields"
                  ? fieldPolicies
                  : auditEvents

    let emptyReason: AccessEmptyReason | undefined
    if (rowsForView.length === 0) {
        emptyReason =
            query.q ||
            query.status ||
            query.risk ||
            query.actorId ||
            query.action
                ? "FILTER_NO_RESULT"
                : "NO_RECORDS_IN_SCOPE"
    }

    const asOf =
        auditEvents[0]?.recordedAt ??
        instantToIso(roles[0]?.created_at) ??
        instantToIso(Math.floor(Date.now() / 1000)) ??
        ""

    const actionBlockers = [
        {
            action: "EXPORT_AUDIT",
            code: "AUDIT_ACCESS_POLICY_MISSING",
            message: "审计访问/导出策略未配置：仅允许保守查询，导出禁用。",
        },
        {
            action: "ASSIGN_USER_ROLE",
            code: "USER_ROLE_TIME_POLICY_MISSING",
            message:
                "用户角色时间策略未配置：仅允许立即紧急撤权，不可预约/到期编辑。",
        },
        {
            action: "UPDATE_FIELD_POLICY",
            code: "FIELD_POLICY_GRANULARITY_MISSING",
            message: "字段粒度策略未配置：字段策略只读，不可提交变更。",
        },
    ]

    void permsPage

    return {
        view: query.view,
        permissionVersion,
        watermark: `w19-${asOf}`,
        calculatedAt: asOf,
        metrics: {
            roleCount: roles.length,
            userCount: admins.length,
            scopeCount: scopesPage.total,
            fieldPolicyCount: 0,
            auditEventCount: auditPage.total,
        },
        governancePolicies: gp,
        emptyReason,
        roles: roleRows,
        users: userRows,
        scopes: scopeRows,
        fieldPolicies,
        auditEvents,
        auditCoverageFrom:
            gp.auditAccessPolicy.state === "MISSING"
                ? gp.auditAccessPolicy.fallbackFrom
                : undefined,
        auditCoverageTo:
            gp.auditAccessPolicy.state === "MISSING"
                ? gp.auditAccessPolicy.fallbackTo
                : undefined,
        allowedActions: ["VIEW_EFFECTIVE_ACCESS", "EMERGENCY_REVOKE_USER_ROLE"],
        actionBlockers,
        workItemSupport: "DISABLED",
    }
}
