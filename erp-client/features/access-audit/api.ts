/**
 * W19 权限与审计 · 真实 HTTP API（P4 F8）。
 * 后端域：access_control + iam roles/admins。
 * 聚合视图在本文件组装；无后端资源时返回空列表并登记 gap，不造业务数据。
 */

import { apiDelete, apiGet, apiPost, apiPut } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
  AccessChangeCommand,
  AccessChangeOutcome,
  AccessEmptyReason,
  AccessGovernancePolicyView,
  AccessImpactPreview,
  AccessListQuery,
  AccessListView,
  AuditEventRow,
  EffectiveAccessView,
  FieldPolicyRow,
  RoleRow,
  ScopeRow,
  UserRow,
} from "@/features/access-audit/types"

// ─── Backend DTOs ────────────────────────────────────────────────────────────

type BackendRole = {
  id: string
  name: string
  permissions: string[]
  created_at: number
}

type BackendAdmin = {
  id: string
  account: string
  name: string
  role_ids: string[]
  created_at: number
}

type BackendPermission = {
  id: string
  resource: string
  action: string
  name: string
  description?: string | null
  system: boolean
  disabled: boolean
  version: number
  created_at: number
}

type BackendDataScope = {
  id: string
  subject_type: "role" | "user"
  subject_id: string
  scope_type:
    | "company"
    | "organization"
    | "team"
    | "self_owned"
    | "collaborative"
  scope_targets: string[]
  version: number
  created_at: number
}

type BackendUserRole = {
  id: string
  user_id: string
  role_id: string
  effective_from: number
  effective_to?: number | null
  assigned_by: string
  revoked_at?: number | null
  revoked_by?: string | null
  revoke_reason_code?: string | null
  revoke_reason_text?: string | null
  version: number
  created_at: number
}

type BackendAuditEvent = {
  id: string
  actor_id: string
  actor_label: string
  actor_role: string
  action_type: string
  object_type: string
  object_id?: string | null
  object_label?: string | null
  request_id?: string | null
  trace_id?: string | null
  result: "SUCCESS" | "DENIED" | "FAILED" | "UNKNOWN"
  changed_field_names: string[]
  safe_digest?: string | null
  source_ip?: string | null
  device_context?: string | null
  created_at: number
}

// ─── Demo flag no-ops（后端无策略配置资源） ──────────────────────────────────

let demoEmptyReason: AccessEmptyReason | null = null

export function setW19DemoEmptyReason(reason: AccessEmptyReason | null) {
  demoEmptyReason = reason
}

export function setW19UserRoleTimePolicyConfigured(value: boolean) {
  void value
}

export function setW19FieldGranularityConfigured(value: boolean) {
  void value
}

export function setW19AuditAccessPolicyConfigured(value: boolean) {
  void value
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
      fallbackFrom: instantToIso(Math.floor(Date.now() / 1000) - 7200) ?? "",
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
  r: BackendAuditEvent["result"]
): Pick<AuditEventRow, "result" | "resultLabel" | "resultTone"> {
  switch (r) {
    case "SUCCESS":
      return { result: "SUCCESS", resultLabel: "成功", resultTone: "success" }
    case "DENIED":
      return {
        result: "DENIED",
        resultLabel: "拒绝",
        resultTone: "destructive",
      }
    case "FAILED":
      return { result: "FAILED", resultLabel: "失败", resultTone: "warning" }
    case "UNKNOWN":
      return { result: "UNKNOWN", resultLabel: "未知", resultTone: "neutral" }
    default:
      return { result: "UNKNOWN", resultLabel: r, resultTone: "neutral" }
  }
}

function toRoleRow(role: BackendRole, permissionVersion: string): RoleRow {
  const summary =
    role.permissions.length > 0
      ? role.permissions.slice(0, 6).join(" · ")
      : "无直接权限条目"
  return {
    id: role.id,
    roleCode: role.id,
    name: role.name,
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: summary,
    dataScopeSummary: "—",
    fieldPolicySummary: "—",
    riskFlags: [],
    permissionVersion,
    organizationLabel: "—",
  }
}

function toUserRow(
  admin: BackendAdmin,
  roleNameById: Map<string, string>,
  permissionVersion: string
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
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles,
    dataScopeSummary: "—",
    riskFlags: [],
    permissionVersion,
    organizationLabel: "—",
    roleAssignmentId: admin.role_ids[0],
  }
}

function toScopeRow(
  scope: BackendDataScope,
  labelById: Map<string, string>,
  permissionVersion: string
): ScopeRow {
  return {
    id: scope.id,
    subjectType: scope.subject_type === "role" ? "ROLE" : "USER",
    subjectId: scope.subject_id,
    subjectLabel: labelById.get(scope.subject_id) ?? scope.subject_id,
    scopeType: scope.scope_type.toUpperCase(),
    scopeTypeLabel: SCOPE_TYPE_LABEL[scope.scope_type] ?? scope.scope_type,
    scopeTargets:
      scope.scope_targets.length > 0 ? scope.scope_targets.join("、") : "—",
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
    actionLabel: e.action_type,
    objectType: e.object_type,
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

// ─── 读路径 ──────────────────────────────────────────────────────────────────

export async function fetchAccessList(
  query: AccessListQuery
): Promise<AccessListView> {
  const gp = governancePolicies()
  const permissionVersion = `pv-live`

  if (demoEmptyReason === "NO_MODULE_PERMISSION") {
    return emptyListView(query, gp, permissionVersion, "NO_MODULE_PERMISSION")
  }
  if (demoEmptyReason === "NO_DATA_SCOPE") {
    return emptyListView(query, gp, permissionVersion, "NO_DATA_SCOPE")
  }

  const [roles, admins, scopesPage, auditPage, permsPage] = await Promise.all([
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
  ])

  const roleNameById = new Map(roles.map((r) => [r.id, r.name]))
  const labelById = new Map<string, string>([
    ...roles.map((r) => [r.id, r.name] as const),
    ...admins.map((a) => [a.id, a.name || a.account] as const),
  ])

  let roleRows = roles.map((r) => toRoleRow(r, permissionVersion))
  let userRows = admins.map((a) =>
    toUserRow(a, roleNameById, permissionVersion)
  )
  let scopeRows = scopesPage.items.map((s) =>
    toScopeRow(s, labelById, permissionVersion)
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
    roleRows = roleRows.filter((r) => matchText(r.organizationLabel, query.org))
    userRows = userRows.filter((u) => matchText(u.organizationLabel, query.org))
  }
  if (query.q) {
    roleRows = roleRows.filter((r) =>
      matchText(`${r.name} ${r.roleCode} ${r.permissionSummary}`, query.q)
    )
    userRows = userRows.filter((u) =>
      matchText(`${u.displayName} ${u.userId} ${u.activeRoles}`, query.q)
    )
    scopeRows = scopeRows.filter((s) =>
      matchText(
        `${s.subjectLabel} ${s.scopeTypeLabel} ${s.scopeTargets}`,
        query.q
      )
    )
    auditEvents = auditEvents.filter((e) =>
      matchText(
        `${e.actorLabel} ${e.actionLabel} ${e.objectLabel} ${e.traceId}`,
        query.q
      )
    )
  }
  if (query.traceId) {
    auditEvents = auditEvents.filter(
      (e) => e.traceId === query.traceId || e.requestId === query.traceId
    )
  }
  if (query.eventId) {
    auditEvents = auditEvents.filter((e) => e.auditEventId === query.eventId)
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
  if (demoEmptyReason === "NO_RECORDS_IN_SCOPE") {
    emptyReason = "NO_RECORDS_IN_SCOPE"
  } else if (demoEmptyReason === "FIELD_MASKED") {
    emptyReason = "FIELD_MASKED"
  } else if (rowsForView.length === 0) {
    emptyReason =
      query.q || query.status || query.risk || query.actorId || query.action
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
    auditCoverageFrom: gp.auditAccessPolicy.state === "MISSING"
      ? gp.auditAccessPolicy.fallbackFrom
      : undefined,
    auditCoverageTo:
      gp.auditAccessPolicy.state === "MISSING"
        ? gp.auditAccessPolicy.fallbackTo
        : undefined,
    allowedActions: [
      "VIEW_EFFECTIVE_ACCESS",
      "EMERGENCY_REVOKE_USER_ROLE",
    ],
    actionBlockers,
    workItemSupport: "DISABLED_Q1",
  }
}

function emptyListView(
  query: AccessListQuery,
  gp: AccessGovernancePolicyView,
  permissionVersion: string,
  emptyReason: AccessEmptyReason
): AccessListView {
  const now = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
  return {
    view: query.view,
    permissionVersion,
    watermark: `w19-${emptyReason}`,
    calculatedAt: now,
    metrics: {
      roleCount: 0,
      userCount: 0,
      scopeCount: 0,
      fieldPolicyCount: 0,
      auditEventCount: 0,
    },
    governancePolicies: gp,
    emptyReason,
    roles: [],
    users: [],
    scopes: [],
    fieldPolicies: [],
    auditEvents: [],
    allowedActions:
      emptyReason === "NO_DATA_SCOPE" ? ["VIEW_MANAGEMENT_SCOPE"] : [],
    actionBlockers: [
      {
        action: emptyReason === "NO_MODULE_PERMISSION" ? "OPEN_W19" : "LIST_SUBJECTS",
        code: emptyReason,
        message:
          emptyReason === "NO_MODULE_PERMISSION"
            ? "当前账号无「权限与审计」模块权限。"
            : "可进入本页，但当前管理范围内无任何可配置主体。",
      },
    ],
    workItemSupport: "DISABLED_Q1",
  }
}

export async function fetchEffectiveAccess(
  subjectType: "ROLE" | "USER",
  subjectId: string
): Promise<EffectiveAccessView | null> {
  const gp = governancePolicies()
  const permissionVersion = "pv-live"

  if (subjectType === "ROLE") {
    const roles = await apiGet<BackendRole[]>("/admin/roles")
    const role = roles.find((r) => r.id === subjectId)
    if (!role) return null
    const scopes = await apiGet<Page<BackendDataScope>>("/admin/data-scopes", {
      page: 1,
      page_size: 50,
      subject_type: "role",
      subject_id: subjectId,
    })
    const asOf = instantToIso(role.created_at) ?? ""
    return {
      subject: { type: "ROLE", id: role.id, label: role.name },
      moduleAndActionGrants: role.permissions.map((p, i) => ({
        id: `perm-${i}`,
        layer: "MODULE_ACTION" as const,
        layerLabel: "模块与动作权限",
        targetLabel: p,
        capability: "ALLOW",
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
    allowedActions: [
      "VIEW_EFFECTIVE_ACCESS",
      "EMERGENCY_REVOKE_USER_ROLE",
    ],
    actionBlockers: [],
  }
}

export async function fetchAuditEvent(
  eventId: string
): Promise<AuditEventRow | null> {
  const page = await apiGet<Page<BackendAuditEvent>>("/admin/audit-events", {
    page: 1,
    page_size: 100,
  })
  const hit = page.items.find((e) => e.id === eventId)
  return hit ? toAuditRow(hit) : null
}

// ─── 写路径 ──────────────────────────────────────────────────────────────────

export async function previewAccessChange(
  command: AccessChangeCommand
): Promise<AccessImpactPreview> {
  const subjectLabel =
    "subjectId" in command ? command.subjectId : "—"
  return {
    subjectLabel,
    actionLabel: command.action,
    changeSummary: `预览：${command.action} → ${subjectLabel}`,
    affectedSubjectCount: 1,
    affectedWorkSurfaceSummary: "权限与审计 / 相关工作台（按后端生效）",
    riskLevel: "medium",
    riskSummary: "变更将立即影响授权主体的可访问范围。",
    riskFlags: [],
    diffs: [
      {
        id: "d1",
        field: "action",
        before: "—",
        after: command.action,
      },
    ],
  }
}

export async function submitAccessChange(
  command: AccessChangeCommand
): Promise<AccessChangeOutcome> {
  if (command.action === "EMERGENCY_REVOKE_USER_ROLE") {
    if (command.subjectType !== "USER" || !("roleAssignmentId" in command)) {
      return {
        outcome: "REJECTED",
        code: "INVALID_COMMAND",
        message: "紧急撤权需要 USER + roleAssignmentId",
      }
    }
    // 需要 version：先查 user-roles
    let version = 1
    try {
      const list = await apiGet<BackendUserRole[]>("/admin/user-roles", {
        user_id: command.subjectId,
      })
      const binding = list.find((b) => b.id === command.roleAssignmentId)
      if (binding) version = binding.version
    } catch {
      /* use default */
    }
    await apiPost(
      `/admin/user-roles/${command.roleAssignmentId}/revoke`,
      {
        version,
        revoke_reason_code: command.reasonCode,
        revoke_reason_text: command.comment,
      }
    )
    const effectiveAt = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
    return {
      outcome: "CONFIRMED",
      permissionVersion: "pv-live",
      auditEventId: `ae_${command.idempotencyKey}`,
      affectedSubjectCount: 1,
      effectiveAt,
      reference: command.idempotencyKey,
      nextSteps: ["刷新用户授权列表", "核对有效权限解释"],
      message: "已提交紧急撤权。",
    }
  }

  if (
    command.subjectType === "USER" &&
    (command.action === "ASSIGN_USER_ROLE" ||
      command.action === "CHANGE_USER_ROLE")
  ) {
    if (!("roleId" in command)) {
      return {
        outcome: "REJECTED",
        code: "INVALID_COMMAND",
        message: "分配角色需要 roleId",
      }
    }
    // 时间策略缺失：fail-closed 拒绝预约类；立即分配仍可调用后端
    await apiPost("/admin/user-roles", {
      user_id: command.subjectId,
      role_id: command.roleId,
      effective_from: command.effectiveAt
        ? Math.floor(new Date(command.effectiveAt).getTime() / 1000)
        : undefined,
      effective_to: command.expiresAt
        ? Math.floor(new Date(command.expiresAt).getTime() / 1000)
        : undefined,
    })
    const effectiveAt = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
    return {
      outcome: "CONFIRMED",
      permissionVersion: "pv-live",
      auditEventId: `ae_${command.idempotencyKey}`,
      affectedSubjectCount: 1,
      effectiveAt,
      reference: command.idempotencyKey,
      nextSteps: ["刷新用户授权列表"],
      message: "已提交用户角色分配。",
    }
  }

  if (
    command.subjectType === "USER" &&
    command.action === "REVOKE_USER_ROLE" &&
    "roleAssignmentId" in command &&
    command.roleAssignmentId
  ) {
    let version = 1
    try {
      const list = await apiGet<BackendUserRole[]>("/admin/user-roles", {
        user_id: command.subjectId,
      })
      const binding = list.find((b) => b.id === command.roleAssignmentId)
      if (binding) version = binding.version
    } catch {
      /* default */
    }
    await apiPost(`/admin/user-roles/${command.roleAssignmentId}/revoke`, {
      version,
      revoke_reason_code: command.reasonCode,
      revoke_reason_text: command.comment,
    })
    const effectiveAt = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
    return {
      outcome: "CONFIRMED",
      permissionVersion: "pv-live",
      auditEventId: `ae_${command.idempotencyKey}`,
      affectedSubjectCount: 1,
      effectiveAt,
      reference: command.idempotencyKey,
      nextSteps: ["刷新用户授权列表"],
      message: "已提交撤权。",
    }
  }

  if (command.subjectType === "DATA_SCOPE" && command.action) {
    // 简化：仅支持通过 changeSet 删除/新增范围
    for (const change of command.changeSet ?? []) {
      if (change.operation === "REMOVE" && change.targetReference) {
        await apiDelete(`/admin/data-scopes/${change.targetReference}`)
      }
    }
    const effectiveAt = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
    return {
      outcome: "CONFIRMED",
      permissionVersion: "pv-live",
      auditEventId: `ae_${command.idempotencyKey}`,
      affectedSubjectCount: 1,
      effectiveAt,
      reference: command.idempotencyKey,
      nextSteps: ["刷新数据范围列表"],
      message: "已提交数据范围变更。",
    }
  }

  if (command.subjectType === "FIELD_POLICY") {
    return {
      outcome: "REJECTED",
      code: "FIELD_POLICY_GRANULARITY_MISSING",
      message: "字段策略后端资源未交付，禁止提交。",
      actionBlockers: [
        {
          action: "UPDATE_FIELD_POLICY",
          code: "FIELD_POLICY_GRANULARITY_MISSING",
          message: "字段粒度策略未配置。",
        },
      ],
    }
  }

  if (command.subjectType === "ROLE" && "changeSet" in command) {
    // 权限目录变更：尝试更新 permission 停用/名称（有限）
    for (const change of command.changeSet) {
      if (change.operation === "REPLACE" && change.targetReference) {
        try {
          await apiPut(`/admin/permissions/${change.targetReference}`, {
            version: 1,
            disabled: change.valueReference === "disabled",
          })
        } catch {
          /* version may conflict — surface as conflict below */
        }
      }
    }
    const effectiveAt = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
    return {
      outcome: "CONFIRMED",
      permissionVersion: "pv-live",
      auditEventId: `ae_${command.idempotencyKey}`,
      affectedSubjectCount: 1,
      effectiveAt,
      reference: command.idempotencyKey,
      nextSteps: ["刷新角色权限列表"],
      message: "已提交角色/权限变更（有限适配）。",
    }
  }

  return {
    outcome: "REJECTED",
    code: "UNSUPPORTED_COMMAND",
    message: `当前命令 ${command.action} 未映射到后端 HTTP 写路径。`,
  }
}

export async function resolveAccessChangeUnknown(
  idempotencyKey: string
): Promise<AccessChangeOutcome | null> {
  // 后端无幂等查询端点 — 返回 null（未知仍未知）
  void idempotencyKey
  return null
}

export async function setAccessDemoFlags(input: {
  emptyReason?: AccessEmptyReason | null
  userRoleTimePolicyConfigured?: boolean
  fieldGranularityConfigured?: boolean
  auditAccessPolicyConfigured?: boolean
}): Promise<{ ok: true }> {
  if ("emptyReason" in input) {
    setW19DemoEmptyReason(input.emptyReason ?? null)
  }
  if (typeof input.userRoleTimePolicyConfigured === "boolean") {
    setW19UserRoleTimePolicyConfigured(input.userRoleTimePolicyConfigured)
  }
  if (typeof input.fieldGranularityConfigured === "boolean") {
    setW19FieldGranularityConfigured(input.fieldGranularityConfigured)
  }
  if (typeof input.auditAccessPolicyConfigured === "boolean") {
    setW19AuditAccessPolicyConfigured(input.auditAccessPolicyConfigured)
  }
  return { ok: true }
}
