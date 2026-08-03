/** W19 权限与审计 · 客户端契约类型 */

export type AccessView =
  | "roles"
  | "users"
  | "scopes"
  | "fields"
  | "audit"

export type AccessEmptyReason =
  | "NO_MODULE_PERMISSION"
  | "NO_DATA_SCOPE"
  | "NO_RECORDS_IN_SCOPE"
  | "FILTER_NO_RESULT"
  | "FIELD_MASKED"

export type AccessListQuery = {
  view: AccessView
  q?: string
  status?: string
  org?: string
  risk?: string
  subjectType?: string
  subjectId?: string
  /** 审计筛选 */
  from?: string
  to?: string
  actorId?: string
  action?: string
  objectType?: string
  objectId?: string
  result?: string
  traceId?: string
  eventId?: string
}

export type ActionBlocker = Readonly<{
  action: string
  code: string
  message: string
}>

export type UserRoleTimePolicy =
  | {
      state: "MISSING"
      allowedActions: readonly ["EMERGENCY_REVOKE_USER_ROLE"]
      blockerCode: "USER_ROLE_TIME_POLICY_MISSING"
    }
  | {
      state: "CONFIGURED"
      policyVersion: string
      schedulingAllowed: boolean
      expirationAllowed: boolean
    }

export type FieldPolicyGranularity =
  | {
      state: "MISSING"
      editable: false
      blockerCode: "FIELD_POLICY_GRANULARITY_MISSING"
    }
  | {
      state: "CONFIGURED"
      policyVersion: string
      editableTargets: readonly {
        policyTargetId: string
        label: string
      }[]
    }

export type AuditAccessPolicy =
  | {
      state: "MISSING"
      fallbackFrom: string
      fallbackTo: string
      configurationExportAllowed: false
      auditExportAllowed: false
      blockerCode: "AUDIT_ACCESS_POLICY_MISSING"
    }
  | {
      state: "CONFIGURED"
      policyVersion: string
      defaultFrom: string
      defaultTo: string
      maxOnlineWindowSeconds: number
      configurationExportThreshold: { maxRows?: number }
      auditExportThreshold: { maxWindowSeconds?: number; maxRows?: number }
    }

export type AccessGovernancePolicyView = Readonly<{
  userRoleTimePolicy: UserRoleTimePolicy
  fieldPolicyGranularity: FieldPolicyGranularity
  auditAccessPolicy: AuditAccessPolicy
}>

export type RoleRow = Readonly<{
  id: string
  roleCode: string
  name: string
  status: "enabled" | "disabled"
  statusLabel: string
  statusTone: "success" | "neutral" | "warning" | "destructive" | "info"
  permissionSummary: string
  dataScopeSummary: string
  fieldPolicySummary: string
  riskFlags: readonly string[]
  permissionVersion: string
  organizationLabel: string
}>

export type UserRow = Readonly<{
  id: string
  userId: string
  displayName: string
  accountStatus: "enabled" | "disabled"
  statusLabel: string
  statusTone: "success" | "neutral" | "warning" | "destructive" | "info"
  activeRoles: string
  /** 已有记录只读展示；策略缺失时不开放编辑 */
  effectiveFrom?: string
  effectiveTo?: string
  dataScopeSummary: string
  riskFlags: readonly string[]
  permissionVersion: string
  organizationLabel: string
  roleAssignmentId?: string
}>

export type ScopeRow = Readonly<{
  id: string
  subjectType: "ROLE" | "USER"
  subjectId: string
  subjectLabel: string
  scopeType: string
  scopeTypeLabel: string
  scopeTargets: string
  permissionVersion: string
  riskFlags: readonly string[]
}>

export type FieldPolicyRow = Readonly<{
  id: string
  policyTargetId: string
  targetLabel: string
  accessCapabilities: readonly string[]
  capabilitySummary: string
  subjectLabel: string
  permissionVersion: string
  editable: boolean
}>

export type AuditEventRow = Readonly<{
  auditEventId: string
  recordedAt: string
  actorId: string
  actorLabel: string
  actorRole: string
  actionType: string
  actionLabel: string
  objectType: string
  objectId: string
  objectLabel: string
  requestId: string
  traceId: string
  result: "SUCCESS" | "DENIED" | "FAILED" | "UNKNOWN"
  resultLabel: string
  resultTone: "success" | "destructive" | "warning" | "neutral" | "info"
  /** 仅字段名；敏感值不返回 */
  changedFieldNames: readonly string[]
  /** 敏感字段变更标记：字段名 +「已变更」 */
  changedFieldDisplay: string
  safeDigest?: string
}>

export type AccessGrantView = Readonly<{
  id: string
  layer: "MODULE_ACTION" | "DATA_SCOPE" | "FIELD" | "HISTORICAL_PARTICIPANT"
  layerLabel: string
  targetLabel: string
  capability: string
  sourceType: string
  sourceLabel: string
  note?: string
}>

export type AccessExplanationView = Readonly<{
  id: string
  code: string
  message: string
  layer: "MODULE_ACTION" | "DATA_SCOPE" | "FIELD" | "OBJECT_STATE"
  layerLabel: string
  sourceType: string
  sourceLabel: string
}>

export type EffectiveAccessView = Readonly<{
  subject: { type: "ROLE" | "USER"; id: string; label: string }
  moduleAndActionGrants: readonly AccessGrantView[]
  dataScopes: readonly AccessGrantView[]
  fieldPolicies: readonly AccessGrantView[]
  historicalParticipantRules: readonly AccessExplanationView[]
  deniedOrBlocked: readonly AccessExplanationView[]
  permissionVersion: string
  calculatedAt: string
  governancePolicies: AccessGovernancePolicyView
  allowedActions: readonly string[]
  actionBlockers: readonly ActionBlocker[]
}>

export type AccessImpactPreview = Readonly<{
  subjectLabel: string
  actionLabel: string
  changeSummary: string
  affectedSubjectCount: number
  affectedWorkSurfaceSummary: string
  riskLevel: "low" | "medium" | "high"
  riskSummary: string
  riskFlags: readonly string[]
  diffs: readonly {
    id: string
    field: string
    before: string
    after: string
    note?: string
  }[]
  /** 服务端返回：是否因 Q1 复核策略未固化而阻断 */
  reviewPolicyBlocker?: ActionBlocker
}>

export type AccessChangeOutcome =
  | {
      outcome: "CONFIRMED"
      permissionVersion: string
      auditEventId: string
      affectedSubjectCount: number
      effectiveAt: string
      reference: string
      nextSteps: readonly string[]
      message: string
    }
  | {
      outcome: "REJECTED"
      code: string
      message: string
      actionBlockers?: readonly ActionBlocker[]
    }
  | {
      outcome: "UNKNOWN"
      message: string
      idempotencyKey: string
    }
  | {
      outcome: "CONFLICT"
      message: string
      serverPermissionVersion: string
    }

export type AccessListMetrics = Readonly<{
  roleCount: number
  userCount: number
  scopeCount: number
  fieldPolicyCount: number
  auditEventCount: number
}>

export type AccessListView = Readonly<{
  view: AccessView
  permissionVersion: string
  watermark: string
  calculatedAt: string
  metrics: AccessListMetrics
  governancePolicies: AccessGovernancePolicyView
  emptyReason?: AccessEmptyReason
  /** 字段掩码演示：列保留、值显示掩码文案 */
  fieldMaskNote?: string
  roles: readonly RoleRow[]
  users: readonly UserRow[]
  scopes: readonly ScopeRow[]
  fieldPolicies: readonly FieldPolicyRow[]
  auditEvents: readonly AuditEventRow[]
  auditCoverageFrom?: string
  auditCoverageTo?: string
  allowedActions: readonly string[]
  actionBlockers: readonly ActionBlocker[]
  /** Q1：本工作面不接收 work_item，无领取/完成入口 */
  workItemSupport: "DISABLED_Q1"
}>

export type AccessChangeCommand =
  | {
      subjectType: "ROLE" | "DATA_SCOPE"
      subjectId: string
      action: string
      expectedPermissionVersion: string
      reasonCode: string
      comment?: string
      idempotencyKey: string
      changeSet: readonly {
        targetReference: string
        operation: "ADD" | "REMOVE" | "REPLACE"
        valueReference?: string
      }[]
    }
  | {
      subjectType: "USER"
      subjectId: string
      action: "EMERGENCY_REVOKE_USER_ROLE"
      roleAssignmentId: string
      expectedPermissionVersion: string
      reasonCode: string
      comment?: string
      idempotencyKey: string
    }
  | {
      subjectType: "USER"
      subjectId: string
      action: "ASSIGN_USER_ROLE" | "CHANGE_USER_ROLE" | "REVOKE_USER_ROLE"
      roleId: string
      roleAssignmentId?: string
      timePolicyVersion: string
      effectiveAt?: string
      expiresAt?: string
      expectedPermissionVersion: string
      reasonCode: string
      comment?: string
      idempotencyKey: string
    }
  | {
      subjectType: "FIELD_POLICY"
      subjectId: string
      action: "UPDATE_FIELD_POLICY"
      granularityPolicyVersion: string
      policyTargetId: string
      accessCapabilities: readonly string[]
      expectedPermissionVersion: string
      reasonCode: string
      comment?: string
      idempotencyKey: string
    }

export const ACCESS_VIEW_LABEL: Record<AccessView, string> = {
  roles: "角色权限",
  users: "用户授权",
  scopes: "数据范围",
  fields: "字段策略",
  audit: "审计查询",
}

export const ACCESS_LAYER_HELP = [
  {
    id: "module",
    title: "模块与动作权限",
    description: "能否进入页面、能否执行某类动作（角色权限与授权策略）。",
  },
  {
    id: "scope",
    title: "数据范围",
    description: "能看哪些客户、团队、组织和单据（data_scope / 协作关系）。",
  },
  {
    id: "field",
    title: "字段权限",
    description: "字段可见、打码、短时查看、编辑或导出到什么程度。",
  },
  {
    id: "object",
    title: "对象状态与业务条件",
    description:
      "单据状态、主责、岗位分离等业务 blocker，不伪装成权限配置问题。",
  },
] as const
