/**
 * W19 session-only mutable state（权限配置变更与审计追加）。
 * Q1 前仅对象级 AccessChange；不创建/领取/完成 work_item。
 */

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

// ── 治理策略（演示：时间/字段粒度默认缺失 fail-closed；审计策略有短窗口 fallback） ──

let userRoleTimePolicyConfigured = false
let fieldGranularityConfigured = false
let auditAccessPolicyConfigured = false

/** Demo toggles — 可在页面内切换演示空态 */
let demoEmptyReason: AccessEmptyReason | null = null

let permissionVersionSeq = 12
function currentPermissionVersion() {
  return `pv-w19-${permissionVersionSeq}`
}

function bumpPermissionVersion() {
  permissionVersionSeq += 1
  return currentPermissionVersion()
}

const idempotencyResults = new Map<string, AccessChangeOutcome>()
const revokedAssignmentIds = new Set<string>()
const disabledRoleIds = new Set<string>()
const fieldPolicyOverrides = new Map<
  string,
  { accessCapabilities: readonly string[]; capabilitySummary: string }
>()
const appendedAudit: AuditEventRow[] = []

function isoMinutesAgo(minutes: number) {
  return new Date(Date.now() - minutes * 60_000).toISOString()
}

function governancePolicies(): AccessGovernancePolicyView {
  const shortFrom = isoMinutesAgo(120)
  const shortTo = new Date().toISOString()

  return {
    userRoleTimePolicy: userRoleTimePolicyConfigured
      ? {
          state: "CONFIGURED",
          policyVersion: "urt-v3",
          schedulingAllowed: true,
          expirationAllowed: true,
        }
      : {
          state: "MISSING",
          allowedActions: ["EMERGENCY_REVOKE_USER_ROLE"],
          blockerCode: "USER_ROLE_TIME_POLICY_MISSING",
        },
    fieldPolicyGranularity: fieldGranularityConfigured
      ? {
          state: "CONFIGURED",
          policyVersion: "fpg-v2",
          editableTargets: [
            { policyTargetId: "pt_bank_account", label: "银行账户字段组" },
            { policyTargetId: "pt_contact_phone", label: "联系方式字段组" },
            { policyTargetId: "pt_id_document", label: "证件信息字段组" },
          ],
        }
      : {
          state: "MISSING",
          editable: false,
          blockerCode: "FIELD_POLICY_GRANULARITY_MISSING",
        },
    auditAccessPolicy: auditAccessPolicyConfigured
      ? {
          state: "CONFIGURED",
          policyVersion: "aap-v1",
          defaultFrom: isoMinutesAgo(24 * 60),
          defaultTo: shortTo,
          maxOnlineWindowSeconds: 7 * 24 * 3600,
          configurationExportThreshold: { maxRows: 5000 },
          auditExportThreshold: { maxWindowSeconds: 30 * 24 * 3600, maxRows: 20000 },
        }
      : {
          state: "MISSING",
          fallbackFrom: shortFrom,
          fallbackTo: shortTo,
          configurationExportAllowed: false,
          auditExportAllowed: false,
          blockerCode: "AUDIT_ACCESS_POLICY_MISSING",
        },
  }
}

// ── Seed data ──

const ROLE_SEED: RoleRow[] = [
  {
    id: "role_sales",
    roleCode: "role.sales",
    name: "销售",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W03 客户 · W04 合同 · W05 销售单（读/建/提交）",
    dataScopeSummary: "本人负责 + 协作参与",
    fieldPolicySummary: "联系方式掩码 · 银行账户隐藏",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "销售中心",
  },
  {
    id: "role_finance_review",
    roleCode: "role.finance_review",
    name: "财务审核",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W11 应收 · W12 应付 · W13 卡资金复核",
    dataScopeSummary: "公司级",
    fieldPolicySummary: "银行账户可见 · 证件掩码",
    riskFlags: ["HIGH_PRIVILEGE"],
    permissionVersion: "pv-w19-12",
    organizationLabel: "财务中心",
  },
  {
    id: "role_warehouse",
    roleCode: "role.warehouse",
    name: "仓储",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W09 履约 · W10 库存台账",
    dataScopeSummary: "指定仓库",
    fieldPolicySummary: "默认字段可见",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "仓储中心",
  },
  {
    id: "role_access_admin",
    roleCode: "role.access_admin",
    name: "权限管理员",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W19 权限配置 · 有效权限解释",
    dataScopeSummary: "被授权组织",
    fieldPolicySummary: "业务敏感值按字段策略裁剪（不可因配置权自动可见）",
    riskFlags: ["HIGH_PRIVILEGE", "ACCESS_ADMIN"],
    permissionVersion: "pv-w19-12",
    organizationLabel: "系统管理",
  },
  {
    id: "role_ops_legacy",
    roleCode: "role.ops_legacy",
    name: "运营（待停用）",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W22 刊登 · W23 执行信息",
    dataScopeSummary: "空数据范围",
    fieldPolicySummary: "默认",
    riskFlags: ["EMPTY_SCOPE", "PENDING_DISABLE"],
    permissionVersion: "pv-w19-12",
    organizationLabel: "运营中心",
  },
  {
    id: "role_audit_viewer",
    roleCode: "role.audit_viewer",
    name: "安全审计",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W19 审计查询（只读）",
    dataScopeSummary: "经授权的审计事件范围",
    fieldPolicySummary: "仅字段名与「已变更」",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "安全",
  },
  {
    id: "role_procurement",
    roleCode: "role.procurement",
    name: "采购",
    status: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    permissionSummary: "W06 采购确认 · W07 采购单",
    dataScopeSummary: "本人负责 + 团队",
    fieldPolicySummary: "供应商账号掩码",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "采购中心",
  },
  {
    id: "role_disabled_demo",
    roleCode: "role.disabled_demo",
    name: "历史角色（已停用）",
    status: "disabled",
    statusLabel: "停用",
    statusTone: "neutral",
    permissionSummary: "无有效模块权限",
    dataScopeSummary: "—",
    fieldPolicySummary: "—",
    riskFlags: [],
    permissionVersion: "pv-w19-10",
    organizationLabel: "系统管理",
  },
]

const USER_SEED: UserRow[] = [
  {
    id: "user_wangmin",
    userId: "user_wangmin",
    displayName: "王敏",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles: "销售",
    effectiveFrom: "2025-01-01T00:00:00.000Z",
    effectiveTo: undefined,
    dataScopeSummary: "销售 · 华东团队",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "销售中心 / 华东",
    roleAssignmentId: "ura_wangmin_sales",
  },
  {
    id: "user_lihua",
    userId: "user_lihua",
    displayName: "李华",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles: "财务审核",
    effectiveFrom: "2025-03-01T00:00:00.000Z",
    dataScopeSummary: "公司级",
    riskFlags: ["HIGH_PRIVILEGE"],
    permissionVersion: "pv-w19-12",
    organizationLabel: "财务中心",
    roleAssignmentId: "ura_lihua_finance",
  },
  {
    id: "user_zhangwei",
    userId: "user_zhangwei",
    displayName: "张伟",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles: "仓储",
    effectiveFrom: "2024-11-15T00:00:00.000Z",
    effectiveTo: "2026-12-31T15:59:59.000Z",
    dataScopeSummary: "华东一仓",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "仓储中心",
    roleAssignmentId: "ura_zhangwei_wh",
  },
  {
    id: "user_chenlei",
    userId: "user_chenlei",
    displayName: "陈磊",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles: "权限管理员",
    effectiveFrom: "2025-06-01T00:00:00.000Z",
    dataScopeSummary: "被授权组织",
    riskFlags: ["ACCESS_ADMIN", "HIGH_PRIVILEGE"],
    permissionVersion: "pv-w19-12",
    organizationLabel: "系统管理",
    roleAssignmentId: "ura_chenlei_admin",
  },
  {
    id: "user_zhoujie",
    userId: "user_zhoujie",
    displayName: "周杰",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles: "采购",
    effectiveFrom: "2025-02-10T00:00:00.000Z",
    dataScopeSummary: "采购 · 华南团队",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "采购中心 / 华南",
    roleAssignmentId: "ura_zhoujie_proc",
  },
  {
    id: "user_sunyue",
    userId: "user_sunyue",
    displayName: "孙悦",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "success",
    activeRoles: "安全审计",
    effectiveFrom: "2025-04-01T00:00:00.000Z",
    dataScopeSummary: "经授权的审计事件",
    riskFlags: [],
    permissionVersion: "pv-w19-12",
    organizationLabel: "安全",
    roleAssignmentId: "ura_sunyue_audit",
  },
  {
    id: "user_temp_ops",
    userId: "user_temp_ops",
    displayName: "临时期权账号",
    accountStatus: "enabled",
    statusLabel: "启用",
    statusTone: "warning",
    activeRoles: "运营（待停用）",
    effectiveFrom: "2025-08-01T00:00:00.000Z",
    effectiveTo: "2025-09-01T00:00:00.000Z",
    dataScopeSummary: "空数据范围",
    riskFlags: ["EMPTY_SCOPE", "EXPIRING_SOON"],
    permissionVersion: "pv-w19-12",
    organizationLabel: "运营中心",
    roleAssignmentId: "ura_temp_ops",
  },
  {
    id: "user_disabled",
    userId: "user_disabled",
    displayName: "已停用账号",
    accountStatus: "disabled",
    statusLabel: "停用",
    statusTone: "neutral",
    activeRoles: "—",
    dataScopeSummary: "—",
    riskFlags: [],
    permissionVersion: "pv-w19-8",
    organizationLabel: "—",
  },
]

const SCOPE_SEED: ScopeRow[] = [
  {
    id: "scope_sales_self",
    subjectType: "ROLE",
    subjectId: "role_sales",
    subjectLabel: "销售",
    scopeType: "OWNER_AND_COLLAB",
    scopeTypeLabel: "本人负责 + 协作",
    scopeTargets: "客户、合同、销售单",
    permissionVersion: "pv-w19-12",
    riskFlags: [],
  },
  {
    id: "scope_finance_company",
    subjectType: "ROLE",
    subjectId: "role_finance_review",
    subjectLabel: "财务审核",
    scopeType: "COMPANY",
    scopeTypeLabel: "公司级",
    scopeTargets: "全部组织资金与结算对象",
    permissionVersion: "pv-w19-12",
    riskFlags: ["HIGH_PRIVILEGE"],
  },
  {
    id: "scope_wh_east",
    subjectType: "ROLE",
    subjectId: "role_warehouse",
    subjectLabel: "仓储",
    scopeType: "WAREHOUSE",
    scopeTypeLabel: "指定仓库",
    scopeTargets: "华东一仓、华东二仓",
    permissionVersion: "pv-w19-12",
    riskFlags: [],
  },
  {
    id: "scope_wangmin_team",
    subjectType: "USER",
    subjectId: "user_wangmin",
    subjectLabel: "王敏",
    scopeType: "TEAM",
    scopeTypeLabel: "团队",
    scopeTargets: "销售 · 华东团队",
    permissionVersion: "pv-w19-12",
    riskFlags: [],
  },
  {
    id: "scope_ops_empty",
    subjectType: "ROLE",
    subjectId: "role_ops_legacy",
    subjectLabel: "运营（待停用）",
    scopeType: "NONE",
    scopeTypeLabel: "空数据范围",
    scopeTargets: "未配置任何目标",
    permissionVersion: "pv-w19-12",
    riskFlags: ["EMPTY_SCOPE"],
  },
  {
    id: "scope_proc_self",
    subjectType: "ROLE",
    subjectId: "role_procurement",
    subjectLabel: "采购",
    scopeType: "OWNER_AND_TEAM",
    scopeTypeLabel: "本人负责 + 团队",
    scopeTargets: "供应商、采购单、履约入站",
    permissionVersion: "pv-w19-12",
    riskFlags: [],
  },
  {
    id: "scope_admin_org",
    subjectType: "ROLE",
    subjectId: "role_access_admin",
    subjectLabel: "权限管理员",
    scopeType: "MANAGED_ORG",
    scopeTypeLabel: "被授权组织",
    scopeTargets: "配置主体（不含业务正文）",
    permissionVersion: "pv-w19-12",
    riskFlags: ["ACCESS_ADMIN"],
  },
  {
    id: "scope_audit_events",
    subjectType: "ROLE",
    subjectId: "role_audit_viewer",
    subjectLabel: "安全审计",
    scopeType: "AUDIT_EVENTS",
    scopeTypeLabel: "审计事件范围",
    scopeTargets: "经授权的追加式审计事件",
    permissionVersion: "pv-w19-12",
    riskFlags: [],
  },
]

const FIELD_POLICY_SEED: FieldPolicyRow[] = [
  {
    id: "fp_bank",
    policyTargetId: "pt_bank_account",
    targetLabel: "银行账户字段组",
    accessCapabilities: ["MASKED", "EXPORTABLE"],
    capabilitySummary: "掩码 · 可导出（再裁剪）",
    subjectLabel: "销售角色默认",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_phone",
    policyTargetId: "pt_contact_phone",
    targetLabel: "联系方式字段组",
    accessCapabilities: ["MASKED", "TEMPORARY_REVEAL"],
    capabilitySummary: "掩码 · 短时揭示",
    subjectLabel: "销售角色默认",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_id",
    policyTargetId: "pt_id_document",
    targetLabel: "证件信息字段组",
    accessCapabilities: ["HIDDEN"],
    capabilitySummary: "隐藏",
    subjectLabel: "销售角色默认",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_finance_bank",
    policyTargetId: "pt_bank_account",
    targetLabel: "银行账户字段组",
    accessCapabilities: ["VISIBLE", "EDITABLE", "EXPORTABLE"],
    capabilitySummary: "可见 · 可编辑 · 可导出",
    subjectLabel: "财务审核角色",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_card_secret",
    policyTargetId: "pt_card_secret",
    targetLabel: "卡密与连接密钥",
    accessCapabilities: ["HIDDEN"],
    capabilitySummary: "禁止展示与导出",
    subjectLabel: "全局强制",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_address",
    policyTargetId: "pt_full_address",
    targetLabel: "完整地址",
    accessCapabilities: ["MASKED"],
    capabilitySummary: "掩码",
    subjectLabel: "仓储角色默认",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_employee_mobile",
    policyTargetId: "pt_employee_mobile",
    targetLabel: "员工手机号",
    accessCapabilities: ["MASKED", "TEMPORARY_REVEAL"],
    capabilitySummary: "掩码 · 短时揭示",
    subjectLabel: "权限管理员视图",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
  {
    id: "fp_audit_digest",
    policyTargetId: "pt_audit_safe_digest",
    targetLabel: "审计安全摘要",
    accessCapabilities: ["VISIBLE"],
    capabilitySummary: "仅摘要引用，不可逆推原值",
    subjectLabel: "安全审计角色",
    permissionVersion: "pv-w19-12",
    editable: false,
  },
]

function buildAuditSeed(): AuditEventRow[] {
  const base: Omit<AuditEventRow, "auditEventId" | "recordedAt" | "traceId" | "requestId">[] =
    [
      {
        actorId: "user_chenlei",
        actorLabel: "陈磊",
        actorRole: "权限管理员",
        actionType: "UPDATE_ROLE_PERMISSIONS",
        actionLabel: "修改模块/动作权限",
        objectType: "ROLE",
        objectId: "role_sales",
        objectLabel: "销售",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: ["modulePermissions"],
        changedFieldDisplay: "modulePermissions · 已变更",
      },
      {
        actorId: "user_chenlei",
        actorLabel: "陈磊",
        actorRole: "权限管理员",
        actionType: "EMERGENCY_REVOKE_USER_ROLE",
        actionLabel: "立即紧急撤权",
        objectType: "USER",
        objectId: "user_temp_ops",
        objectLabel: "临时期权账号",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: ["activeRoles"],
        changedFieldDisplay: "activeRoles · 已变更",
      },
      {
        actorId: "user_wangmin",
        actorLabel: "王敏",
        actorRole: "销售",
        actionType: "VIEW_CUSTOMER_SENSITIVE",
        actionLabel: "短时揭示敏感字段",
        objectType: "CUSTOMER",
        objectId: "cust_10086",
        objectLabel: "华东贸易有限公司",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: ["contactPhone"],
        changedFieldDisplay: "contactPhone · 已变更",
        safeDigest: "sd:a9f3…c21",
      },
      {
        actorId: "user_lihua",
        actorLabel: "李华",
        actorRole: "财务审核",
        actionType: "EXPORT_RECEIVABLE",
        actionLabel: "导出应收明细",
        objectType: "RECEIVABLE_EXPORT",
        objectId: "job_rx_991",
        objectLabel: "应收导出任务 #991",
        result: "DENIED",
        resultLabel: "拒绝",
        resultTone: "destructive",
        changedFieldNames: [],
        changedFieldDisplay: "—",
      },
      {
        actorId: "user_chenlei",
        actorLabel: "陈磊",
        actorRole: "权限管理员",
        actionType: "UPDATE_FIELD_POLICY",
        actionLabel: "修改字段策略",
        objectType: "FIELD_POLICY",
        objectId: "pt_bank_account",
        objectLabel: "银行账户字段组",
        result: "FAILED",
        resultLabel: "失败",
        resultTone: "warning",
        changedFieldNames: ["accessCapabilities"],
        changedFieldDisplay: "accessCapabilities · 已变更",
      },
      {
        actorId: "user_sunyue",
        actorLabel: "孙悦",
        actorRole: "安全审计",
        actionType: "QUERY_AUDIT",
        actionLabel: "查询审计事件",
        objectType: "AUDIT_QUERY",
        objectId: "aq_batch_12",
        objectLabel: "审计查询批次",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: [],
        changedFieldDisplay: "—",
      },
      {
        actorId: "user_zhangwei",
        actorLabel: "张伟",
        actorRole: "仓储",
        actionType: "CREATE_ADJUSTMENT",
        actionLabel: "创建库存调整",
        objectType: "STOCK_ADJUSTMENT",
        objectId: "adj_778",
        objectLabel: "调整单 ADJ-778",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: ["onHandQuantity"],
        changedFieldDisplay: "onHandQuantity · 已变更",
      },
      {
        actorId: "user_chenlei",
        actorLabel: "陈磊",
        actorRole: "权限管理员",
        actionType: "MANAGE_DATA_SCOPE",
        actionLabel: "修改数据范围",
        objectType: "DATA_SCOPE",
        objectId: "scope_wh_east",
        objectLabel: "仓储 · 指定仓库",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: ["scopeTargets"],
        changedFieldDisplay: "scopeTargets · 已变更",
      },
      {
        actorId: "system",
        actorLabel: "系统",
        actorRole: "—",
        actionType: "PERMISSION_VERSION_BUMP",
        actionLabel: "权限版本推进",
        objectType: "PERMISSION",
        objectId: "pv-w19-12",
        objectLabel: "权限版本 pv-w19-12",
        result: "SUCCESS",
        resultLabel: "成功",
        resultTone: "success",
        changedFieldNames: ["permissionVersion"],
        changedFieldDisplay: "permissionVersion · 已变更",
      },
      {
        actorId: "user_zhoujie",
        actorLabel: "周杰",
        actorRole: "采购",
        actionType: "OPEN_SUPPLIER",
        actionLabel: "打开供应商",
        objectType: "SUPPLIER",
        objectId: "sup_55",
        objectLabel: "华南供应联营",
        result: "DENIED",
        resultLabel: "拒绝",
        resultTone: "destructive",
        changedFieldNames: [],
        changedFieldDisplay: "—",
      },
    ]

  return base.map((row, i) => ({
    ...row,
    auditEventId: `ae_seed_${String(i + 1).padStart(3, "0")}`,
    recordedAt: isoMinutesAgo(15 + i * 11),
    requestId: `req_seed_${String(i + 1).padStart(4, "0")}`,
    traceId: `tr_seed_${String(i + 1).padStart(4, "0")}`,
  }))
}

const AUDIT_SEED = buildAuditSeed()

function matchText(hay: string, q?: string) {
  if (!q?.trim()) return true
  return hay.toLowerCase().includes(q.trim().toLowerCase())
}

function projectRoles(query: AccessListQuery): RoleRow[] {
  const pv = currentPermissionVersion()
  return ROLE_SEED.map((r) => {
    const disabled = disabledRoleIds.has(r.id) || r.status === "disabled"
    return {
      ...r,
      status: disabled ? ("disabled" as const) : r.status,
      statusLabel: disabled ? "停用" : r.statusLabel,
      statusTone: disabled ? ("neutral" as const) : r.statusTone,
      permissionVersion: pv,
    }
  }).filter((r) => {
    if (query.status === "enabled" && r.status !== "enabled") return false
    if (query.status === "disabled" && r.status !== "disabled") return false
    if (query.risk && !r.riskFlags.includes(query.risk)) return false
    if (query.org && !matchText(r.organizationLabel, query.org)) return false
    return matchText(`${r.name} ${r.roleCode} ${r.permissionSummary}`, query.q)
  })
}

function projectUsers(query: AccessListQuery): UserRow[] {
  const pv = currentPermissionVersion()
  return USER_SEED.map((u) => {
    if (u.roleAssignmentId && revokedAssignmentIds.has(u.roleAssignmentId)) {
      return {
        ...u,
        activeRoles: "—（已紧急撤权）",
        riskFlags: [...u.riskFlags.filter((f) => f !== "EXPIRING_SOON"), "REVOKED"],
        permissionVersion: pv,
        roleAssignmentId: undefined,
      }
    }
    return { ...u, permissionVersion: pv }
  }).filter((u) => {
    if (query.status === "enabled" && u.accountStatus !== "enabled") return false
    if (query.status === "disabled" && u.accountStatus !== "disabled") return false
    if (query.risk && !u.riskFlags.includes(query.risk)) return false
    if (query.org && !matchText(u.organizationLabel, query.org)) return false
    if (query.subjectId && u.userId !== query.subjectId) return false
    return matchText(
      `${u.displayName} ${u.userId} ${u.activeRoles}`,
      query.q
    )
  })
}

function projectScopes(query: AccessListQuery): ScopeRow[] {
  const pv = currentPermissionVersion()
  return SCOPE_SEED.map((s) => ({ ...s, permissionVersion: pv })).filter(
    (s) => {
      if (query.subjectType && s.subjectType !== query.subjectType) return false
      if (query.subjectId && s.subjectId !== query.subjectId) return false
      if (query.risk && !s.riskFlags.includes(query.risk)) return false
      return matchText(
        `${s.subjectLabel} ${s.scopeTypeLabel} ${s.scopeTargets}`,
        query.q
      )
    }
  )
}

function projectFieldPolicies(query: AccessListQuery): FieldPolicyRow[] {
  const pv = currentPermissionVersion()
  const gp = governancePolicies()
  const editable =
    gp.fieldPolicyGranularity.state === "CONFIGURED" &&
    gp.fieldPolicyGranularity.editableTargets.length > 0

  return FIELD_POLICY_SEED.map((fp) => {
    const override = fieldPolicyOverrides.get(fp.id)
    return {
      ...fp,
      ...(override ?? {}),
      permissionVersion: pv,
      editable:
        editable &&
        gp.fieldPolicyGranularity.state === "CONFIGURED" &&
        gp.fieldPolicyGranularity.editableTargets.some(
          (t) => t.policyTargetId === fp.policyTargetId
        ),
    }
  }).filter((fp) =>
    matchText(`${fp.targetLabel} ${fp.subjectLabel} ${fp.capabilitySummary}`, query.q)
  )
}

function projectAudit(query: AccessListQuery): {
  rows: AuditEventRow[]
  coverageFrom: string
  coverageTo: string
} {
  const gp = governancePolicies()
  const policy = gp.auditAccessPolicy
  const coverageFrom =
    policy.state === "CONFIGURED" ? policy.defaultFrom : policy.fallbackFrom
  const coverageTo =
    policy.state === "CONFIGURED" ? policy.defaultTo : policy.fallbackTo

  const from = query.from ?? coverageFrom
  const to = query.to ?? coverageTo

  const all = [...appendedAudit, ...AUDIT_SEED]
  const rows = all.filter((e) => {
    if (e.recordedAt < from || e.recordedAt > to) return false
    if (query.actorId && e.actorId !== query.actorId) return false
    if (query.action && e.actionType !== query.action) return false
    if (query.objectType && e.objectType !== query.objectType) return false
    if (query.objectId && e.objectId !== query.objectId) return false
    if (query.result && e.result !== query.result) return false
    if (query.traceId && e.traceId !== query.traceId && e.requestId !== query.traceId)
      return false
    if (query.eventId && e.auditEventId !== query.eventId) return false
    return matchText(
      `${e.actorLabel} ${e.actionLabel} ${e.objectLabel} ${e.traceId} ${e.actorRole}`,
      query.q
    )
  })

  return { rows, coverageFrom: from, coverageTo: to }
}

export function setW19DemoEmptyReason(reason: AccessEmptyReason | null) {
  demoEmptyReason = reason
}

export function getW19DemoEmptyReason() {
  return demoEmptyReason
}

export function setW19UserRoleTimePolicyConfigured(value: boolean) {
  userRoleTimePolicyConfigured = value
}

export function setW19FieldGranularityConfigured(value: boolean) {
  fieldGranularityConfigured = value
}

export function setW19AuditAccessPolicyConfigured(value: boolean) {
  auditAccessPolicyConfigured = value
}

export function getW19GovernanceFlags() {
  return {
    userRoleTimePolicyConfigured,
    fieldGranularityConfigured,
    auditAccessPolicyConfigured,
  }
}

export function buildW19ListView(query: AccessListQuery): AccessListView {
  const gp = governancePolicies()
  const pv = currentPermissionVersion()
  const now = new Date().toISOString()

  if (demoEmptyReason === "NO_MODULE_PERMISSION") {
    return {
      view: query.view,
      permissionVersion: pv,
      watermark: `w19-audit-${permissionVersionSeq}`,
      calculatedAt: now,
      metrics: {
        roleCount: 0,
        userCount: 0,
        scopeCount: 0,
        fieldPolicyCount: 0,
        auditEventCount: 0,
      },
      governancePolicies: gp,
      emptyReason: "NO_MODULE_PERMISSION",
      roles: [],
      users: [],
      scopes: [],
      fieldPolicies: [],
      auditEvents: [],
      allowedActions: [],
      actionBlockers: [
        {
          action: "OPEN_W19",
          code: "NO_MODULE_PERMISSION",
          message: "当前账号无「权限与审计」模块权限，入口应在导航中隐藏。",
        },
      ],
      workItemSupport: "DISABLED_Q1",
    }
  }

  if (demoEmptyReason === "NO_DATA_SCOPE") {
    return {
      view: query.view,
      permissionVersion: pv,
      watermark: `w19-audit-${permissionVersionSeq}`,
      calculatedAt: now,
      metrics: {
        roleCount: 0,
        userCount: 0,
        scopeCount: 0,
        fieldPolicyCount: 0,
        auditEventCount: 0,
      },
      governancePolicies: gp,
      emptyReason: "NO_DATA_SCOPE",
      roles: [],
      users: [],
      scopes: [],
      fieldPolicies: [],
      auditEvents: [],
      allowedActions: ["VIEW_MANAGEMENT_SCOPE"],
      actionBlockers: [
        {
          action: "LIST_SUBJECTS",
          code: "NO_DATA_SCOPE",
          message: "可进入本页，但当前管理范围内无任何可配置主体。",
        },
      ],
      workItemSupport: "DISABLED_Q1",
    }
  }

  const roles = projectRoles(query)
  const users = projectUsers(query)
  const scopes = projectScopes(query)
  const fieldPolicies = projectFieldPolicies(query)
  const audit = projectAudit(query)

  let emptyReason: AccessEmptyReason | undefined
  if (demoEmptyReason === "NO_RECORDS_IN_SCOPE") {
    emptyReason = "NO_RECORDS_IN_SCOPE"
  } else if (demoEmptyReason === "FIELD_MASKED") {
    emptyReason = "FIELD_MASKED"
  } else {
    const rowsForView =
      query.view === "roles"
        ? roles
        : query.view === "users"
          ? users
          : query.view === "scopes"
            ? scopes
            : query.view === "fields"
              ? fieldPolicies
              : audit.rows
    if (rowsForView.length === 0) {
      emptyReason =
        query.q ||
        query.status ||
        query.risk ||
        query.actorId ||
        query.action ||
        query.traceId
          ? "FILTER_NO_RESULT"
          : "NO_RECORDS_IN_SCOPE"
    }
  }

  const exportAllowed =
    gp.auditAccessPolicy.state === "CONFIGURED" &&
    (query.view === "audit"
      ? gp.auditAccessPolicy.auditExportThreshold !== undefined
      : true)

  const allowedActions: string[] = [
    "VIEW_EFFECTIVE_ACCESS",
    "EMERGENCY_REVOKE_USER_ROLE",
  ]
  if (exportAllowed && gp.auditAccessPolicy.state === "CONFIGURED") {
    if (query.view === "audit") allowedActions.push("EXPORT_AUDIT")
    else allowedActions.push("EXPORT_CONFIGURATION")
  }

  const actionBlockers: Array<{ action: string; code: string; message: string }> = []
  if (gp.auditAccessPolicy.state === "MISSING") {
    actionBlockers.push({
      action: "EXPORT_AUDIT",
      code: "AUDIT_ACCESS_POLICY_MISSING",
      message: "审计访问/导出策略未配置：仅允许服务端保守短窗口查询，全部导出禁用。",
    })
    actionBlockers.push({
      action: "EXPORT_CONFIGURATION",
      code: "AUDIT_ACCESS_POLICY_MISSING",
      message: "配置导出策略未配置，导出已禁用。",
    })
  }
  if (gp.userRoleTimePolicy.state === "MISSING") {
    actionBlockers.push({
      action: "ASSIGN_USER_ROLE",
      code: "USER_ROLE_TIME_POLICY_MISSING",
      message: "用户角色时间策略未配置：仅允许立即紧急撤权，不可预约/到期编辑。",
    })
  }
  if (gp.fieldPolicyGranularity.state === "MISSING") {
    actionBlockers.push({
      action: "UPDATE_FIELD_POLICY",
      code: "FIELD_POLICY_GRANULARITY_MISSING",
      message: "字段粒度策略未配置：字段策略只读，不可提交变更。",
    })
  }
  // Q1：命中复核的高风险动作失败关闭，不创建 work_item
  actionBlockers.push({
    action: "EXPAND_COMPANY_SCOPE",
    code: "REVIEW_POLICY_UNCONFIGURED",
    message:
      "Q1 复核策略未固化：扩大全公司数据范围等需双人复核的动作已阻断，不创建临时任务。",
  })

  return {
    view: query.view,
    permissionVersion: pv,
    watermark: `w19-audit-${permissionVersionSeq}`,
    calculatedAt: now,
    metrics: {
      roleCount: ROLE_SEED.length - disabledRoleIds.size,
      userCount: USER_SEED.filter((u) => u.accountStatus === "enabled").length,
      scopeCount: SCOPE_SEED.length,
      fieldPolicyCount: FIELD_POLICY_SEED.length,
      auditEventCount: AUDIT_SEED.length + appendedAudit.length,
    },
    governancePolicies: gp,
    emptyReason,
    fieldMaskNote:
      emptyReason === "FIELD_MASKED"
        ? "字段级隐藏：标签与列保留，值按策略掩码（如银行账号显示为 ****）。权限管理员不因此看到业务敏感正文。"
        : undefined,
    roles: emptyReason === "NO_RECORDS_IN_SCOPE" ? [] : roles,
    users: emptyReason === "NO_RECORDS_IN_SCOPE" ? [] : users,
    scopes: emptyReason === "NO_RECORDS_IN_SCOPE" ? [] : scopes,
    fieldPolicies: emptyReason === "NO_RECORDS_IN_SCOPE" ? [] : fieldPolicies,
    auditEvents: emptyReason === "NO_RECORDS_IN_SCOPE" ? [] : audit.rows,
    auditCoverageFrom: audit.coverageFrom,
    auditCoverageTo: audit.coverageTo,
    allowedActions,
    actionBlockers,
    workItemSupport: "DISABLED_Q1",
  }
}

export function getW19EffectiveAccess(
  subjectType: "ROLE" | "USER",
  subjectId: string
): EffectiveAccessView | null {
  const gp = governancePolicies()
  const pv = currentPermissionVersion()
  const now = new Date().toISOString()

  if (subjectType === "ROLE") {
    const role = ROLE_SEED.find((r) => r.id === subjectId)
    if (!role) return null
    return {
      subject: { type: "ROLE", id: role.id, label: role.name },
      moduleAndActionGrants: [
        {
          id: "g1",
          layer: "MODULE_ACTION",
          layerLabel: "模块与动作权限",
          targetLabel: role.permissionSummary,
          capability: "按角色权限矩阵授予",
          sourceType: "ROLE_PERMISSION",
          sourceLabel: `角色 ${role.roleCode}`,
        },
      ],
      dataScopes: [
        {
          id: "g2",
          layer: "DATA_SCOPE",
          layerLabel: "数据范围",
          targetLabel: role.dataScopeSummary,
          capability: "范围内可读",
          sourceType: "DATA_SCOPE",
          sourceLabel: `角色数据范围 · ${role.name}`,
        },
      ],
      fieldPolicies: [
        {
          id: "g3",
          layer: "FIELD",
          layerLabel: "字段权限",
          targetLabel: role.fieldPolicySummary,
          capability: "按字段策略裁剪",
          sourceType: "FIELD_POLICY",
          sourceLabel: "角色默认字段策略",
        },
      ],
      historicalParticipantRules: [
        {
          id: "h1",
          code: "DOCUMENT_PARTICIPANT",
          message:
            "历史参与者查看权来自 formal document_participant，不因当前负责人变更被静默抹去。",
          layer: "DATA_SCOPE",
          layerLabel: "数据范围",
          sourceType: "DOCUMENT_PARTICIPANT",
          sourceLabel: "历史参与关系",
        },
      ],
      deniedOrBlocked: [
        {
          id: "b1",
          code: "OBJECT_STATE",
          message:
            "对象状态/岗位分离等业务条件以各对象 allowedActions / actionBlockers 表达，不在此合并为「没权限」。",
          layer: "OBJECT_STATE",
          layerLabel: "对象状态与业务条件",
          sourceType: "OBJECT_ALLOWED_ACTIONS",
          sourceLabel: "对象中心数据",
        },
        ...(role.riskFlags.includes("EMPTY_SCOPE")
          ? [
              {
                id: "b2",
                code: "EMPTY_DATA_SCOPE",
                message: "该角色数据范围为空：有模块入口时仍可能看不到任何业务记录。",
                layer: "DATA_SCOPE" as const,
                layerLabel: "数据范围",
                sourceType: "DATA_SCOPE",
                sourceLabel: role.name,
              },
            ]
          : []),
      ],
      permissionVersion: pv,
      calculatedAt: now,
      governancePolicies: gp,
      allowedActions:
        role.status === "disabled" || disabledRoleIds.has(role.id)
          ? ["VIEW_EFFECTIVE_ACCESS"]
          : role.riskFlags.includes("HIGH_PRIVILEGE")
            ? ["VIEW_EFFECTIVE_ACCESS", "UPDATE_ROLE_PERMISSIONS"]
            : [
                "VIEW_EFFECTIVE_ACCESS",
                "UPDATE_ROLE_PERMISSIONS",
                "DISABLE_ROLE",
                "MANAGE_DATA_SCOPE",
              ],
      actionBlockers: [
        ...(role.riskFlags.includes("HIGH_PRIVILEGE")
          ? [
              {
                action: "EXPAND_SENSITIVE_FIELD",
                code: "REVIEW_POLICY_UNCONFIGURED",
                message:
                  "Q1 未固化：扩大敏感字段访问需双人复核，当前失败关闭且不创建 work_item。",
              },
            ]
          : []),
        ...(disabledRoleIds.has(role.id) || role.status === "disabled"
          ? [
              {
                action: "UPDATE_ROLE_PERMISSIONS",
                code: "ROLE_DISABLED",
                message: "角色已停用，历史身份保留，不可再改权限矩阵。",
              },
            ]
          : []),
      ],
    }
  }

  const user = USER_SEED.find((u) => u.userId === subjectId)
  if (!user) return null
  const revoked =
    user.roleAssignmentId && revokedAssignmentIds.has(user.roleAssignmentId)

  return {
    subject: { type: "USER", id: user.userId, label: user.displayName },
    moduleAndActionGrants: revoked
      ? []
      : [
          {
            id: "ug1",
            layer: "MODULE_ACTION",
            layerLabel: "模块与动作权限",
            targetLabel: `经角色「${user.activeRoles}」继承`,
            capability: "模块入口与动作",
            sourceType: "USER_ROLE",
            sourceLabel: user.activeRoles,
          },
        ],
    dataScopes: [
      {
        id: "ug2",
        layer: "DATA_SCOPE",
        layerLabel: "数据范围",
        targetLabel: user.dataScopeSummary,
        capability: "范围内可读",
        sourceType: "DATA_SCOPE",
        sourceLabel: user.organizationLabel,
      },
    ],
    fieldPolicies: [
      {
        id: "ug3",
        layer: "FIELD",
        layerLabel: "字段权限",
        targetLabel: "继承角色字段策略；权限管理员不可见业务敏感正文",
        capability: "掩码 / 隐藏",
        sourceType: "FIELD_POLICY",
        sourceLabel: "角色默认",
      },
    ],
    historicalParticipantRules: [
      {
        id: "uh1",
        code: "DOCUMENT_PARTICIPANT",
        message: "曾作为协作人的单据在负责人变更后仍可按参与关系查看。",
        layer: "DATA_SCOPE",
        layerLabel: "数据范围",
        sourceType: "DOCUMENT_PARTICIPANT",
        sourceLabel: "历史参与",
      },
    ],
    deniedOrBlocked: [
      ...(revoked
        ? [
            {
              id: "ub0",
              code: "ROLE_REVOKED",
              message: "用户角色已立即紧急撤权，模块授权已失效。",
              layer: "MODULE_ACTION" as const,
              layerLabel: "模块与动作权限",
              sourceType: "EMERGENCY_REVOKE",
              sourceLabel: "紧急撤权",
            },
          ]
        : []),
      {
        id: "ub1",
        code: "OBJECT_STATE",
        message: "具体单据能否操作取决于对象状态与岗位分离，不在此页伪装为配置缺失。",
        layer: "OBJECT_STATE",
        layerLabel: "对象状态与业务条件",
        sourceType: "OBJECT_ALLOWED_ACTIONS",
        sourceLabel: "对象中心",
      },
    ],
    permissionVersion: pv,
    calculatedAt: now,
    governancePolicies: gp,
    allowedActions: revoked
      ? ["VIEW_EFFECTIVE_ACCESS"]
      : gp.userRoleTimePolicy.state === "MISSING"
        ? ["VIEW_EFFECTIVE_ACCESS", "EMERGENCY_REVOKE_USER_ROLE"]
        : [
            "VIEW_EFFECTIVE_ACCESS",
            "EMERGENCY_REVOKE_USER_ROLE",
            "ASSIGN_USER_ROLE",
            "CHANGE_USER_ROLE",
          ],
    actionBlockers:
      gp.userRoleTimePolicy.state === "MISSING"
        ? [
            {
              action: "ASSIGN_USER_ROLE",
              code: "USER_ROLE_TIME_POLICY_MISSING",
              message: "时间策略未配置：不可分配/预约/到期编辑用户角色。",
            },
            {
              action: "CHANGE_USER_ROLE",
              code: "USER_ROLE_TIME_POLICY_MISSING",
              message: "时间策略未配置：仅可立即紧急撤权。",
            },
          ]
        : [],
  }
}

export function getW19AuditEvent(eventId: string): AuditEventRow | null {
  return (
    appendedAudit.find((e) => e.auditEventId === eventId) ??
    AUDIT_SEED.find((e) => e.auditEventId === eventId) ??
    null
  )
}

export function previewW19AccessChange(
  command: AccessChangeCommand
): AccessImpactPreview {
  const gp = governancePolicies()

  if (command.action === "EMERGENCY_REVOKE_USER_ROLE") {
    const user = USER_SEED.find((u) => u.userId === command.subjectId)
    return {
      subjectLabel: user?.displayName ?? command.subjectId,
      actionLabel: "立即紧急撤权",
      changeSummary: `立即撤销授权记录 ${"roleAssignmentId" in command ? command.roleAssignmentId : "—"}，敏感会话缓存失效。`,
      affectedSubjectCount: 1,
      affectedWorkSurfaceSummary: "该用户已打开的工作面将按新权限版本失效查询缓存",
      riskLevel: "high",
      riskSummary: "立即止损：角色组合失效，不依赖时间策略。",
      riskFlags: ["IMMEDIATE", "SESSION_INVALIDATION"],
      diffs: [
        {
          id: "d1",
          field: "activeRoles",
          before: user?.activeRoles ?? "—",
          after: "—（已撤权）",
          note: "仅展示授权结果摘要，不含敏感业务值",
        },
      ],
    }
  }

  if (command.action === "UPDATE_FIELD_POLICY") {
    if (gp.fieldPolicyGranularity.state === "MISSING") {
      return {
        subjectLabel: command.subjectId,
        actionLabel: "修改字段策略",
        changeSummary: "字段粒度策略未配置，无法预览可写变更。",
        affectedSubjectCount: 0,
        affectedWorkSurfaceSummary: "—",
        riskLevel: "high",
        riskSummary: "FIELD_POLICY_GRANULARITY_MISSING",
        riskFlags: ["POLICY_MISSING"],
        diffs: [],
        reviewPolicyBlocker: {
          action: "UPDATE_FIELD_POLICY",
          code: "FIELD_POLICY_GRANULARITY_MISSING",
          message: "字段粒度策略未配置，字段策略只读。",
        },
      }
    }
    const caps =
      "accessCapabilities" in command ? command.accessCapabilities.join(" · ") : "—"
    return {
      subjectLabel: "policyTargetId" in command ? command.policyTargetId : command.subjectId,
      actionLabel: "修改字段策略",
      changeSummary: `策略目标 ${"policyTargetId" in command ? command.policyTargetId : "—"} 访问能力调整为 ${caps}`,
      affectedSubjectCount: 24,
      affectedWorkSurfaceSummary: "客户/合同/资金工作面字段裁剪",
      riskLevel: "medium",
      riskSummary: "字段能力变更将使已揭示敏感值在权限版本推进后清除。",
      riskFlags: ["FIELD_CAPABILITY"],
      diffs: [
        {
          id: "d1",
          field: "accessCapabilities",
          before: "（当前系统数据）",
          after: caps,
        },
      ],
    }
  }

  if (
    command.action === "ASSIGN_USER_ROLE" ||
    command.action === "CHANGE_USER_ROLE" ||
    command.action === "REVOKE_USER_ROLE"
  ) {
    if (gp.userRoleTimePolicy.state === "MISSING") {
      return {
        subjectLabel: command.subjectId,
        actionLabel: "用户角色变更",
        changeSummary: "时间策略未配置，非紧急撤权动作不可提交。",
        affectedSubjectCount: 0,
        affectedWorkSurfaceSummary: "—",
        riskLevel: "high",
        riskSummary: "USER_ROLE_TIME_POLICY_MISSING",
        riskFlags: ["POLICY_MISSING"],
        diffs: [],
        reviewPolicyBlocker: {
          action: command.action,
          code: "USER_ROLE_TIME_POLICY_MISSING",
          message: "时间策略未配置：仅紧急撤权可提交。",
        },
      }
    }
  }

  if (command.action === "DISABLE_ROLE") {
    const role = ROLE_SEED.find((r) => r.id === command.subjectId)
    return {
      subjectLabel: role?.name ?? command.subjectId,
      actionLabel: "停用角色",
      changeSummary: "角色停用后历史身份保留；任务责任池需替代角色。",
      affectedSubjectCount: 8,
      affectedWorkSurfaceSummary: "依赖该角色的任务池与入口",
      riskLevel: "high",
      riskSummary: "停用不删除历史角色代码；需确认无不可停用 blocker。",
      riskFlags: ["ROLE_DISABLE", "TASK_POOL"],
      diffs: [
        {
          id: "d1",
          field: "status",
          before: "启用",
          after: "停用",
        },
      ],
    }
  }

  // 高风险扩权 → Q1 阻断
  if (command.action === "EXPAND_COMPANY_SCOPE" || command.action === "UPDATE_ROLE_PERMISSIONS") {
    const role = ROLE_SEED.find((r) => r.id === command.subjectId)
    const highRisk =
      command.action === "EXPAND_COMPANY_SCOPE" ||
      role?.riskFlags.includes("HIGH_PRIVILEGE")
    return {
      subjectLabel: role?.name ?? command.subjectId,
      actionLabel:
        command.action === "EXPAND_COMPANY_SCOPE"
          ? "扩大全公司数据范围"
          : "修改模块/动作权限",
      changeSummary:
        "changeSet" in command
          ? `变更项 ${command.changeSet.length} 条`
          : "权限矩阵调整",
      affectedSubjectCount: highRisk ? 146 : 32,
      affectedWorkSurfaceSummary: "相关模块入口与动作",
      riskLevel: highRisk ? "high" : "medium",
      riskSummary: highRisk
        ? "高风险扩权：Q1 复核策略未固化时服务端将阻断。"
        : "权限缩减可按服务端策略直接生效。",
      riskFlags: highRisk ? ["HIGH_PRIVILEGE", "NEEDS_REVIEW"] : ["PERMISSION_CHANGE"],
      diffs:
        "changeSet" in command
          ? command.changeSet.map((c, i) => ({
              id: `c${i}`,
              field: c.targetReference,
              before: c.operation === "ADD" ? "未授予" : "已授予",
              after: c.operation === "REMOVE" ? "移除" : c.valueReference ?? c.operation,
            }))
          : [],
      reviewPolicyBlocker: highRisk
        ? {
            action: command.action,
            code: "REVIEW_POLICY_UNCONFIGURED",
            message:
              "Q1 决策前：命中复核要求的动作失败关闭，不创建/领取/完成 work_item。",
          }
        : undefined,
    }
  }

  return {
    subjectLabel: command.subjectId,
    actionLabel: command.action,
    changeSummary: "对象级 AccessChange 预览",
    affectedSubjectCount: 1,
    affectedWorkSurfaceSummary: "—",
    riskLevel: "low",
    riskSummary: "常规配置变更",
    riskFlags: [],
    diffs: [],
  }
}

function appendAuditEvent(
  partial: Omit<
    AuditEventRow,
    "auditEventId" | "recordedAt" | "requestId" | "traceId"
  >
) {
  const seq = appendedAudit.length + 1
  const row: AuditEventRow = {
    ...partial,
    auditEventId: `ae_live_${Date.now().toString(36)}_${seq}`,
    recordedAt: new Date().toISOString(),
    requestId: `req_live_${Date.now().toString(36)}`,
    traceId: `tr_live_${Date.now().toString(36)}`,
  }
  appendedAudit.unshift(row)
  return row
}

export function submitW19AccessChange(
  command: AccessChangeCommand
): AccessChangeOutcome {
  const existing = idempotencyResults.get(command.idempotencyKey)
  if (existing) return existing

  const gp = governancePolicies()
  const currentPv = currentPermissionVersion()

  if (command.expectedPermissionVersion !== currentPv) {
    const result: AccessChangeOutcome = {
      outcome: "CONFLICT",
      message: "权限版本已变化，禁止静默覆盖。请基于最新版本重新预览并提交。",
      serverPermissionVersion: currentPv,
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  // 字段策略
  if (command.action === "UPDATE_FIELD_POLICY") {
    if (gp.fieldPolicyGranularity.state === "MISSING") {
      const result: AccessChangeOutcome = {
        outcome: "REJECTED",
        code: "FIELD_POLICY_GRANULARITY_MISSING",
        message: "字段粒度策略未配置，字段策略只读。",
        actionBlockers: [
          {
            action: "UPDATE_FIELD_POLICY",
            code: "FIELD_POLICY_GRANULARITY_MISSING",
            message: "请先配置字段粒度策略；配置后仅可提交 policyTargetId + 版本。",
          },
        ],
      }
      idempotencyResults.set(command.idempotencyKey, result)
      return result
    }
    if (
      !("policyTargetId" in command) ||
      !gp.fieldPolicyGranularity.editableTargets.some(
        (t) => t.policyTargetId === command.policyTargetId
      )
    ) {
      const result: AccessChangeOutcome = {
        outcome: "REJECTED",
        code: "INVALID_POLICY_TARGET",
        message: "只能提交服务端返回的 policyTargetId，禁止自由字段路径。",
      }
      idempotencyResults.set(command.idempotencyKey, result)
      return result
    }
    if (
      "granularityPolicyVersion" in command &&
      command.granularityPolicyVersion !== gp.fieldPolicyGranularity.policyVersion
    ) {
      const result: AccessChangeOutcome = {
        outcome: "CONFLICT",
        message: "字段粒度策略版本已变化。",
        serverPermissionVersion: currentPv,
      }
      idempotencyResults.set(command.idempotencyKey, result)
      return result
    }

    const row = FIELD_POLICY_SEED.find(
      (f) => f.policyTargetId === command.policyTargetId
    )
    if (row && "accessCapabilities" in command) {
      fieldPolicyOverrides.set(row.id, {
        accessCapabilities: command.accessCapabilities,
        capabilitySummary: command.accessCapabilities.join(" · "),
      })
    }
    const newPv = bumpPermissionVersion()
    const audit = appendAuditEvent({
      actorId: "current_user",
      actorLabel: "当前用户",
      actorRole: "权限管理员",
      actionType: "UPDATE_FIELD_POLICY",
      actionLabel: "修改字段策略",
      objectType: "FIELD_POLICY",
      objectId: command.policyTargetId,
      objectLabel: row?.targetLabel ?? command.policyTargetId,
      result: "SUCCESS",
      resultLabel: "成功",
      resultTone: "success",
      changedFieldNames: ["accessCapabilities"],
      changedFieldDisplay: "accessCapabilities · 已变更",
    })
    const result: AccessChangeOutcome = {
      outcome: "CONFIRMED",
      permissionVersion: newPv,
      auditEventId: audit.auditEventId,
      affectedSubjectCount: 24,
      effectiveAt: audit.recordedAt,
      reference: `ACC-${audit.auditEventId}`,
      nextSteps: [
        "各工作面按新 permissionVersion 失效查询缓存",
        "已揭示敏感值应立即清除",
        "可在审计查询中打开本事件",
      ],
      message: "字段策略已更新。",
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  // 用户角色：时间策略缺失时仅紧急撤权
  if (
    command.action === "ASSIGN_USER_ROLE" ||
    command.action === "CHANGE_USER_ROLE" ||
    command.action === "REVOKE_USER_ROLE"
  ) {
    if (gp.userRoleTimePolicy.state === "MISSING") {
      const result: AccessChangeOutcome = {
        outcome: "REJECTED",
        code: "USER_ROLE_TIME_POLICY_MISSING",
        message: "用户角色时间策略未配置：仅允许立即紧急撤权。",
        actionBlockers: [
          {
            action: command.action,
            code: "USER_ROLE_TIME_POLICY_MISSING",
            message: "页面不得展示预约/到期编辑控件。",
          },
        ],
      }
      idempotencyResults.set(command.idempotencyKey, result)
      return result
    }
  }

  if (command.action === "EMERGENCY_REVOKE_USER_ROLE") {
    if (!("roleAssignmentId" in command) || !command.roleAssignmentId) {
      const result: AccessChangeOutcome = {
        outcome: "REJECTED",
        code: "MISSING_ASSIGNMENT",
        message: "缺少 roleAssignmentId。",
      }
      idempotencyResults.set(command.idempotencyKey, result)
      return result
    }
    // 禁止携带预约/到期（类型层 never；运行时再防一手）
    if (
      "effectiveAt" in command ||
      "expiresAt" in command ||
      "timePolicyVersion" in command
    ) {
      const result: AccessChangeOutcome = {
        outcome: "REJECTED",
        code: "INVALID_EMERGENCY_PAYLOAD",
        message: "紧急撤权不得携带预约/到期或时间策略版本字段。",
      }
      idempotencyResults.set(command.idempotencyKey, result)
      return result
    }

    revokedAssignmentIds.add(command.roleAssignmentId)
    const newPv = bumpPermissionVersion()
    const user = USER_SEED.find((u) => u.userId === command.subjectId)
    const audit = appendAuditEvent({
      actorId: "current_user",
      actorLabel: "当前用户",
      actorRole: "权限管理员",
      actionType: "EMERGENCY_REVOKE_USER_ROLE",
      actionLabel: "立即紧急撤权",
      objectType: "USER",
      objectId: command.subjectId,
      objectLabel: user?.displayName ?? command.subjectId,
      result: "SUCCESS",
      resultLabel: "成功",
      resultTone: "success",
      changedFieldNames: ["activeRoles"],
      changedFieldDisplay: "activeRoles · 已变更",
    })
    const result: AccessChangeOutcome = {
      outcome: "CONFIRMED",
      permissionVersion: newPv,
      auditEventId: audit.auditEventId,
      affectedSubjectCount: 1,
      effectiveAt: audit.recordedAt,
      reference: `ACC-${audit.auditEventId}`,
      nextSteps: [
        "该用户会话敏感缓存应立即失效",
        "打开中工作面按新权限版本重查",
        "审计事件已追加，不可编辑删除",
      ],
      message: "已立即撤销指定用户角色授权。",
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  // Q1 高风险复核阻断
  if (
    command.action === "EXPAND_COMPANY_SCOPE" ||
    (command.action === "UPDATE_ROLE_PERMISSIONS" &&
      ROLE_SEED.find((r) => r.id === command.subjectId)?.riskFlags.includes(
        "HIGH_PRIVILEGE"
      ))
  ) {
    const result: AccessChangeOutcome = {
      outcome: "REJECTED",
      code: "REVIEW_POLICY_UNCONFIGURED",
      message:
        "Q1 复核策略未固化：本动作失败关闭。W19 不创建、领取或完成 work_item。",
      actionBlockers: [
        {
          action: command.action,
          code: "REVIEW_POLICY_UNCONFIGURED",
          message: "待注册固定 work_item_type 并复用 W02 CompleteWorkItemEnvelope。",
        },
      ],
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  if (command.action === "DISABLE_ROLE") {
    disabledRoleIds.add(command.subjectId)
    const newPv = bumpPermissionVersion()
    const role = ROLE_SEED.find((r) => r.id === command.subjectId)
    const audit = appendAuditEvent({
      actorId: "current_user",
      actorLabel: "当前用户",
      actorRole: "权限管理员",
      actionType: "DISABLE_ROLE",
      actionLabel: "停用角色",
      objectType: "ROLE",
      objectId: command.subjectId,
      objectLabel: role?.name ?? command.subjectId,
      result: "SUCCESS",
      resultLabel: "成功",
      resultTone: "success",
      changedFieldNames: ["status"],
      changedFieldDisplay: "status · 已变更",
    })
    const result: AccessChangeOutcome = {
      outcome: "CONFIRMED",
      permissionVersion: newPv,
      auditEventId: audit.auditEventId,
      affectedSubjectCount: 8,
      effectiveAt: audit.recordedAt,
      reference: `ACC-${audit.auditEventId}`,
      nextSteps: [
        "历史角色代码与身份保留",
        "检查任务责任池替代角色",
        "按新权限版本刷新相关工作面",
      ],
      message: "角色已停用。",
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  // 一般角色权限缩减（非高风险）直接生效
  if (command.action === "UPDATE_ROLE_PERMISSIONS") {
    const newPv = bumpPermissionVersion()
    const role = ROLE_SEED.find((r) => r.id === command.subjectId)
    const audit = appendAuditEvent({
      actorId: "current_user",
      actorLabel: "当前用户",
      actorRole: "权限管理员",
      actionType: "UPDATE_ROLE_PERMISSIONS",
      actionLabel: "修改模块/动作权限",
      objectType: "ROLE",
      objectId: command.subjectId,
      objectLabel: role?.name ?? command.subjectId,
      result: "SUCCESS",
      resultLabel: "成功",
      resultTone: "success",
      changedFieldNames: ["modulePermissions"],
      changedFieldDisplay: "modulePermissions · 已变更",
    })
    const result: AccessChangeOutcome = {
      outcome: "CONFIRMED",
      permissionVersion: newPv,
      auditEventId: audit.auditEventId,
      affectedSubjectCount: 32,
      effectiveAt: audit.recordedAt,
      reference: `ACC-${audit.auditEventId}`,
      nextSteps: [
        "配置版本已更新",
        "受影响用户工作面缓存失效",
        "可查看有效权限解释确认来源",
      ],
      message: "角色权限已更新。",
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  if (command.action === "MANAGE_DATA_SCOPE") {
    const newPv = bumpPermissionVersion()
    const audit = appendAuditEvent({
      actorId: "current_user",
      actorLabel: "当前用户",
      actorRole: "权限管理员",
      actionType: "MANAGE_DATA_SCOPE",
      actionLabel: "修改数据范围",
      objectType: "DATA_SCOPE",
      objectId: command.subjectId,
      objectLabel: command.subjectId,
      result: "SUCCESS",
      resultLabel: "成功",
      resultTone: "success",
      changedFieldNames: ["scopeTargets"],
      changedFieldDisplay: "scopeTargets · 已变更",
    })
    const result: AccessChangeOutcome = {
      outcome: "CONFIRMED",
      permissionVersion: newPv,
      auditEventId: audit.auditEventId,
      affectedSubjectCount: 12,
      effectiveAt: audit.recordedAt,
      reference: `ACC-${audit.auditEventId}`,
      nextSteps: ["客户端收到权限版本变化后重查", "范围目标名称按当前范围显示"],
      message: "数据范围已更新。",
    }
    idempotencyResults.set(command.idempotencyKey, result)
    return result
  }

  const result: AccessChangeOutcome = {
    outcome: "REJECTED",
    code: "UNSUPPORTED_ACTION",
    message: `当前演示不支持动作 ${command.action}；且禁止通过 W19 创建 work_item。`,
  }
  idempotencyResults.set(command.idempotencyKey, result)
  return result
}

export function queryW19Idempotency(
  idempotencyKey: string
): AccessChangeOutcome | null {
  return idempotencyResults.get(idempotencyKey) ?? null
}
