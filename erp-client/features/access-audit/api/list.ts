import { personScopeRows } from "./person-scopes"
// 人员范围仅在人员和范围视图读取，角色管理不依赖人员范围配置权限。
// field policies 后端无资源：返回空列表并登记 gap，不造业务数据。

import { fetchCompleteList } from "@/lib/collect-pages"
import { auditKeywordActions } from "@/features/access-audit/lib/audit-labels"
import { isPermissionDeniedError } from "@/lib/api/errors"
import { apiGet } from "@/lib/api"
import type {
    AccessEmptyReason,
    AccessListQuery,
    AccessListView,
    FieldPolicyRow,
} from "@/features/access-audit/types"
import type {
    BackendAdmin,
    BackendAuditEvent,
    BackendRole,
} from "./backend-types"
import {
    governancePolicies,
    SCOPE_TYPE_LABEL,
    instantToIso,
    matchText,
    toAuditRow,
    toRoleRow,
    toScopeRow,
    toUserRow,
} from "./mappers"

export async function fetchAccessList(
    query: AccessListQuery,
    includeAccounts: boolean,
): Promise<AccessListView> {
    const gp = governancePolicies()
    const permissionVersion = `pv-live`

    // 权限审计读取保存的事件快照，不依赖人员或角色管理权限。
    const [roles, admins, auditPage] = await Promise.all([
        query.view === "audit"
            ? Promise.resolve<BackendRole[]>([])
            : apiGet<BackendRole[]>("/admin/roles"),
        query.view === "audit" || !includeAccounts
            ? Promise.resolve(null)
            : apiGet<BackendAdmin[]>("/admin/admins").catch(
                  (error: unknown) => {
                      if (
                          query.view === "roles" &&
                          isPermissionDeniedError(error)
                      )
                          return null
                      throw error
                  },
              ),
        query.view !== "audit"
            ? Promise.resolve({ items: [] as BackendAuditEvent[], total: 0 })
            : fetchCompleteList<BackendAuditEvent>("/admin/audit-events", {
                  q: query.q?.trim() || undefined,
                  keyword_actions: auditKeywordActions(query.q),
                  event_id: query.eventId,
                  trace_id: query.traceId,
                  created_from: auditDate(query.from),
                  created_before: auditDate(query.to, true),
                  page: 1,
                  page_size: 1,
                  actor_id: query.actorId,
                  action_type: query.action,
                  object_id: query.objectId,
                  result: query.result as
                      | BackendAuditEvent["result"]
                      | undefined,
              }),
    ])

    const scopeItems =
        (query.view !== "users" && query.view !== "scopes") ||
        query.subjectType === "ROLE"
            ? []
            : await personScopeRows(
                  query.subjectId
                      ? [query.subjectId]
                      : (admins ?? []).map((a) => a.id),
              )
    const scopesPage = { items: scopeItems, total: scopeItems.length }
    const roleNameById = new Map(roles.map((r) => [r.id, r.name]))
    const labelById = new Map<string, string>([
        ...roles.map((r) => [r.id, r.name] as const),
        ...(admins ?? []).map((a) => [a.id, a.name || a.account] as const),
    ])

    // 人员范围只关联账号，不从角色继承。
    const scopeTextBySubject = new Map<string, string[]>()
    for (const scope of scopesPage.items) {
        const label = scope.summary
            ? scope.summary
            : scope.scope_targets.length > 0
              ? `${SCOPE_TYPE_LABEL[scope.scope_type] ?? scope.scope_type}（${scope.scope_targets.join("、")}）`
              : (SCOPE_TYPE_LABEL[scope.scope_type] ?? scope.scope_type)
        const bucket = scopeTextBySubject.get(scope.subject_id)
        if (bucket) bucket.push(label)
        else scopeTextBySubject.set(scope.subject_id, [label])
    }
    const scopeSummaryFor = (...subjectIds: string[]) => {
        const labels = [
            ...new Set(
                subjectIds.flatMap((id) => scopeTextBySubject.get(id) ?? []),
            ),
        ]
        return labels.length > 0 ? labels.join(" · ") : "—"
    }
    const boundAccountsByRole = new Map<string, number>()
    for (const admin of admins ?? []) {
        for (const roleId of admin.role_ids) {
            boundAccountsByRole.set(
                roleId,
                (boundAccountsByRole.get(roleId) ?? 0) + 1,
            )
        }
    }

    let roleRows = roles.map((r) =>
        toRoleRow(
            r,
            permissionVersion,
            admins ? (boundAccountsByRole.get(r.id) ?? 0) : null,
            scopeSummaryFor(r.id),
        ),
    )
    let userRows = (admins ?? []).map((a) =>
        toUserRow(a, roleNameById, permissionVersion, scopeSummaryFor(a.id)),
    )
    let scopeRows = scopesPage.items.map((s) =>
        toScopeRow(s, labelById, permissionVersion),
    )
    // field policies：后端无 field_policy 资源
    const fieldPolicies: FieldPolicyRow[] = []
    const auditEvents = auditPage.items.map(toAuditRow)

    if (query.q) {
        roleRows = roleRows.filter((r) =>
            matchText(
                `${r.name} ${r.roleCode} ${r.permissionSummary}`,
                query.q,
            ),
        )
        userRows = userRows.filter((u) =>
            matchText(
                `${u.displayName} ${u.accountName} ${u.activeRoles}`,
                query.q,
            ),
        )
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
            query.q || query.actorId || query.action || query.result
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
            message: "暂未开启操作记录导出功能。",
        },
        {
            action: "EXPORT_CONFIGURATION",
            code: "AUDIT_ACCESS_POLICY_MISSING",
            message: "暂未开启权限配置导出功能。",
        },
        {
            action: "ASSIGN_USER_ROLE",
            code: "USER_ROLE_TIME_POLICY_MISSING",
            message:
                "暂不支持设置角色生效或到期时间，仅支持立即调整或紧急撤权。",
        },
        {
            action: "UPDATE_FIELD_POLICY",
            code: "FIELD_POLICY_GRANULARITY_MISSING",
            message: "暂不支持按字段单独设置权限，当前仅支持查看。",
        },
    ]

    return {
        view: query.view,
        permissionVersion,
        watermark: `w19-${asOf}`,
        calculatedAt: asOf,
        metrics: {
            roleCount: roles.length,
            userCount: admins?.length ?? 0,
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
        allowedActions: ["VIEW_EFFECTIVE_ACCESS"],
        actionBlockers,
        workItemSupport: "DISABLED",
    }
}

/** 以上海业务日构建包含开始、不包含次日零点的日期边界。 */
function auditDate(
    date: string | undefined,
    nextDay = false,
): number | undefined {
    if (!date) return undefined
    const time = Date.parse(`${date.slice(0, 10)}T00:00:00+08:00`)
    return Number.isFinite(time)
        ? Math.floor(time / 1000) + (nextDay ? 86400 : 0)
        : undefined
}
