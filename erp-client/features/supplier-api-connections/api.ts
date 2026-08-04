/**
 * W20 session-mock API
 * - 密钥永不返回正文
 * - 角色分离：采购确认需求 vs 运维配置引用 vs 管理员启停/能力
 * - 健康检查 / 目录同步返回后台任务
 * - 正式结果未知可按幂等键查询
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  CapabilityCode,
  ConnectionCenterView,
  ConnectionListView,
  ConnectionStatus,
  DemoRole,
  FormalOutcome,
  HealthResult,
  ReferenceState,
} from "@/features/supplier-api-connections/types"
import {
  CAPABILITY_LABEL,
  CATALOG_LABEL,
  DEMO_ROLE_LABEL,
  ENVIRONMENT_LABEL,
  HEALTH_LABEL,
  HEALTH_TONE,
  REFERENCE_STATE_LABEL,
  STATUS_LABEL,
  STATUS_TONE,
} from "@/features/supplier-api-connections/types"
import {
  CREDENTIAL_OPAQUE_OPTIONS,
  SEED_CONNECTIONS,
  seedToListItem,
  type SeedConnection,
} from "@/mock/supplier-api-connections"
import {
  getIdempotencyEntry,
  setIdempotencyPending,
  setIdempotencySucceeded,
} from "@/mock/session-state"
import { resultText } from "@/lib/ui-text"

const WORKSPACE = "W20"

type SessionOverlay = {
  status?: ConnectionStatus
  version: string
  endpoint?: SeedConnection["endpoint"]
  credential?: SeedConnection["credential"]
  capabilities?: SeedConnection["capabilities"]
  lastHealth?: SeedConnection["lastHealth"]
  healthRecords?: SeedConnection["healthRecords"]
  catalog?: SeedConnection["catalog"]
  alerts?: SeedConnection["alerts"]
  nextStep?: string
  auditEvents?: SeedConnection["auditEvents"]
}

const overlays = new Map<string, SessionOverlay>()
const createdConnections: SeedConnection[] = []
const activeJobs = new Map<
  string,
  {
    jobId: string
    jobNo: string
    kind: "HEALTH" | "CATALOG"
    connectionId: string
    status: "queued" | "running" | "succeeded" | "partial" | "failed"
    total: number
    completed: number
    succeeded: number
    failed: number
    createdAt: string
  }
>()

function getOverlay(id: string): SessionOverlay | undefined {
  return overlays.get(id)
}

function bumpVersion(current: string): string {
  const n = Number(current)
  return Number.isFinite(n) ? String(n + 1) : `${current}.1`
}

function mergeSeed(seed: SeedConnection): SeedConnection {
  const o = getOverlay(seed.connectionId)
  if (!o) return seed
  return {
    ...seed,
    status: o.status ?? seed.status,
    version: o.version || seed.version,
    endpoint: o.endpoint ?? seed.endpoint,
    credential: o.credential ?? seed.credential,
    capabilities: o.capabilities ?? seed.capabilities,
    lastHealth: o.lastHealth ?? seed.lastHealth,
    healthRecords: o.healthRecords ?? seed.healthRecords,
    catalog: o.catalog ?? seed.catalog,
    alerts: o.alerts ?? seed.alerts,
    nextStep: o.nextStep ?? seed.nextStep,
    auditEvents: o.auditEvents ?? seed.auditEvents,
  }
}

function allSeeds(): SeedConnection[] {
  return [...createdConnections, ...SEED_CONNECTIONS].map(mergeSeed)
}

function findSeed(connectionId: string): SeedConnection | undefined {
  return allSeeds().find((s) => s.connectionId === connectionId)
}

function roleActions(
  role: DemoRole,
  seed: SeedConnection
): { allowed: string[]; blockers: ConnectionCenterView["actionBlockers"] } {
  const allowed: string[] = ["OPEN_CENTER", "VIEW"]
  const blockers: ConnectionCenterView["actionBlockers"] = []

  const refsReady =
    seed.endpoint.state !== "MISSING" && seed.credential.state !== "MISSING"
  const hasEnabledCap = seed.capabilities.some((c) => c.status === "ENABLED")

  if (role === "procurement") {
    allowed.push("CONFIRM_CAPABILITY_REQUIREMENT", "EDIT_BUSINESS_PROFILE")
    if (seed.catalog.state !== "NEVER") {
      allowed.push("OPEN_W21")
    }
    blockers.push(
      {
        action: "BIND_CREDENTIAL_REFERENCE",
        code: "ROLE_PROCUREMENT",
        message: "采购不可绑定/轮换密钥引用；请联系研发运维",
      },
      {
        action: "UPDATE_CAPABILITIES",
        code: "ROLE_PROCUREMENT",
        message: "采购只能确认业务能力需求，不能写能力启停状态",
      },
      {
        action: "ENABLE",
        code: "ROLE_PROCUREMENT",
        message: "启用连接由系统管理员执行",
      },
      {
        action: "DISABLE",
        code: "ROLE_PROCUREMENT",
        message: "停用连接由系统管理员执行",
      },
      {
        action: "RUN_HEALTH_CHECK",
        code: "ROLE_PROCUREMENT",
        message: "健康检查由研发运维执行",
      }
    )
  } else if (role === "ops") {
    allowed.push(
      "BIND_ENDPOINT_REFERENCE",
      "BIND_CREDENTIAL_REFERENCE",
      "RUN_HEALTH_CHECK",
      "OPEN_W29"
    )
    if (hasEnabledCap && seed.status === "ENABLED") {
      allowed.push("START_CATALOG_SYNC", "OPEN_W21")
    }
    blockers.push(
      {
        action: "CONFIRM_CAPABILITY_REQUIREMENT",
        code: "ROLE_OPS",
        message: "研发运维不能代替采购确认业务能力需求",
      },
      {
        action: "UPDATE_CAPABILITIES",
        code: "ROLE_OPS",
        message: "能力启停仅系统管理员可配置",
      },
      {
        action: "ENABLE",
        code: "ROLE_OPS",
        message: "启用连接由系统管理员执行（需采购与运维前置完成）",
      },
      {
        action: "DISABLE",
        code: "ROLE_OPS",
        message: "停用连接由系统管理员执行",
      }
    )
    if (!refsReady) {
      blockers.push({
        action: "RUN_HEALTH_CHECK",
        code: "REFS_NOT_READY",
        message: "地址或密钥引用未就绪，无法执行健康检查",
      })
      allowed.splice(allowed.indexOf("RUN_HEALTH_CHECK"), 1)
    }
  } else {
    // admin
    allowed.push(
      "CREATE_CONNECTION",
      "UPDATE_CAPABILITIES",
      "ENABLE",
      "DISABLE",
      "RUN_HEALTH_CHECK",
      "BIND_ENDPOINT_REFERENCE",
      "BIND_CREDENTIAL_REFERENCE",
      "START_CATALOG_SYNC",
      "OPEN_W21",
      "OPEN_W29",
      "OPEN_W19"
    )
    blockers.push({
      action: "CONFIRM_CAPABILITY_REQUIREMENT",
      code: "ROLE_ADMIN",
      message:
        "系统管理员不能代替采购确认业务能力需求；采购确认与能力配置无共用写入口",
    })
    if (!refsReady || !hasEnabledCap) {
      if (seed.status !== "ENABLED") {
        blockers.push({
          action: "ENABLE",
          code: "PRECONDITIONS",
          message: "启用前需完成引用绑定、能力配置与健康验证",
        })
      }
    }
    if (seed.status === "ENABLED" && seed.relatedImpact.openSupplierOrders > 0) {
      // still allow disable but impact preview will show
    }
  }

  if (seed.lastHealth?.result === "AUTH_FAILED") {
    blockers.push({
      action: "START_CATALOG_SYNC",
      code: "AUTH_FAILED",
      message: "鉴权失败期间禁止触发目录同步；请先轮换引用并健康验证",
    })
  }

  if (seed.lastHealth?.result === "UNKNOWN") {
    blockers.push({
      action: "RUN_HEALTH_CHECK",
      code: "PRIOR_UNKNOWN",
      message: "存在结果未知的健康任务，请先按任务号查询，勿盲目重复发起",
    })
  }

  return { allowed: [...new Set(allowed)], blockers }
}

function projectSafeRefs(
  seed: SeedConnection,
  role: DemoRole
): ConnectionCenterView["safeReferences"] {
  const tech = role === "ops" || role === "admin"
  const mapRef = (r: SeedConnection["endpoint"]) => ({
    state: r.state,
    alias: tech ? r.alias : undefined,
    version: tech ? r.version : undefined,
    visible: tech,
  })
  return {
    endpoint: mapRef(seed.endpoint),
    credential: mapRef(seed.credential),
  }
}

function toCenter(
  seed: SeedConnection,
  role: DemoRole
): ConnectionCenterView {
  const { allowed, blockers } = roleActions(role, seed)
  const adapterVisible = role === "ops" || role === "admin"
  return {
    connectionId: seed.connectionId,
    connectionCode: seed.connectionCode,
    supplier: seed.supplier,
    environment: seed.environment,
    environmentLabel: ENVIRONMENT_LABEL[seed.environment],
    status: seed.status,
    statusLabel: STATUS_LABEL[seed.status],
    statusTone: STATUS_TONE[seed.status],
    businessOwner: seed.businessOwner,
    technicalOwner: seed.technicalOwner,
    adapter: seed.adapter
      ? { ...seed.adapter, visible: adapterVisible }
      : undefined,
    version: seed.version,
    updatedAt: seed.updatedAt,
    safeReferences: projectSafeRefs(seed, role),
    capabilities: seed.capabilities.map((c) => ({
      ...c,
      productLevelNote:
        "连接级能力声明 ≠ 每个商品可用；商品/供给/发布级能力见供应商商品库 / 商品发布",
    })),
    lastHealth:
      role === "procurement" && seed.lastHealth
        ? {
            at: seed.lastHealth.at,
            result: seed.lastHealth.result,
            resultLabel: seed.lastHealth.resultLabel,
            autoRetryStopped: seed.lastHealth.autoRetryStopped,
            errorSummary: seed.lastHealth.errorSummary
              ? seed.lastHealth.errorSummary.replace(/密钥|secret|key/gi, "引用")
              : undefined,
          }
        : seed.lastHealth,
    healthRecords:
      role === "procurement"
        ? seed.healthRecords.map((h) => ({
            ...h,
            latencyMs: undefined,
            errorClass: undefined,
          }))
        : seed.healthRecords,
    catalog: seed.catalog,
    relatedImpact: seed.relatedImpact,
    auditEvents: seed.auditEvents,
    allowedActions: allowed,
    actionBlockers: blockers,
    alerts: seed.alerts ?? [],
    nextStep: seed.nextStep,
  }
}

export type ListQueryInput = {
  environment: string
  status?: string
  health?: string
  capability?: string
  catalogFreshness?: string
  supplierId?: string
  q?: string
  page: number
  pageSize?: number
  role: DemoRole
  demoFlag?: "no-permission" | "no-scope"
}

export async function fetchConnectionList(
  input: ListQueryInput
): Promise<ConnectionListView> {
  await mockDelay()

  if (input.demoFlag === "no-permission") {
    return {
      metrics: {
        enabled: 0,
        faulted: 0,
        pendingConfig: 0,
        healthAbnormal: 0,
        catalogStale: 0,
      },
      items: [],
      total: 0,
      page: 1,
      pageSize: input.pageSize ?? 20,
      emptyReason: "NO_PERMISSION",
      viewerRole: input.role,
      viewerRoleLabel: DEMO_ROLE_LABEL[input.role],
      hasModulePermission: false,
      hasDataScope: false,
      projectedAt: new Date().toISOString(),
      credentialOpaqueOptions: [],
    }
  }

  if (input.demoFlag === "no-scope") {
    return {
      metrics: {
        enabled: 0,
        faulted: 0,
        pendingConfig: 0,
        healthAbnormal: 0,
        catalogStale: 0,
      },
      items: [],
      total: 0,
      page: 1,
      pageSize: input.pageSize ?? 20,
      emptyReason: "NO_SCOPE",
      viewerRole: input.role,
      viewerRoleLabel: DEMO_ROLE_LABEL[input.role],
      hasModulePermission: true,
      hasDataScope: false,
      projectedAt: new Date().toISOString(),
      credentialOpaqueOptions: [],
    }
  }

  let seeds = allSeeds()
  const env = input.environment.toUpperCase()
  if (env !== "ALL") {
    seeds = seeds.filter((s) => s.environment === env)
  }

  // metrics before status filters that would hide counts? Doc: metrics clickable sync filters.
  // Compute metrics on env-filtered set.
  const metricsBase = seeds
  const metrics = {
    enabled: metricsBase.filter((s) => s.status === "ENABLED").length,
    faulted: metricsBase.filter((s) => s.status === "FAULTED").length,
    pendingConfig: metricsBase.filter((s) => s.status === "PENDING_CONFIG")
      .length,
    healthAbnormal: metricsBase.filter((s) => {
      const r = s.lastHealth?.result
      return (
        r === "FAILED" ||
        r === "AUTH_FAILED" ||
        r === "PARTIAL" ||
        r === "UNKNOWN"
      )
    }).length,
    catalogStale: metricsBase.filter(
      (s) => s.catalog.state === "STALE" || s.catalog.state === "FAILED"
    ).length,
  }

  if (input.status) {
    const statuses = input.status.split(",").map((s) => s.trim().toUpperCase())
    seeds = seeds.filter((s) => statuses.includes(s.status))
  } else {
    // default: ENABLED + FAULTED (doc §6)
    seeds = seeds.filter(
      (s) =>
        s.status === "ENABLED" ||
        s.status === "FAULTED" ||
        s.status === "PENDING_CONFIG"
    )
  }

  if (input.health) {
    const hs = input.health.split(",").map((h) => h.trim().toUpperCase())
    seeds = seeds.filter((s) => {
      const r = s.lastHealth?.result ?? "UNCHECKED"
      return hs.includes(r)
    })
  }

  if (input.capability) {
    const code = input.capability.toUpperCase() as CapabilityCode
    seeds = seeds.filter((s) =>
      s.capabilities.some(
        (c) => c.capabilityCode === code && c.status === "ENABLED"
      )
    )
  }

  if (input.catalogFreshness) {
    const cs = input.catalogFreshness
      .split(",")
      .map((c) => c.trim().toUpperCase())
    seeds = seeds.filter((s) => cs.includes(s.catalog.state))
  }

  if (input.supplierId) {
    seeds = seeds.filter((s) => s.supplier.id === input.supplierId)
  }

  if (input.q?.trim()) {
    const q = input.q.trim().toUpperCase()
    seeds = seeds.filter(
      (s) =>
        s.connectionCode.toUpperCase().includes(q) ||
        s.supplier.name.toUpperCase().includes(q)
    )
  }

  // fault first, then last health desc
  seeds = [...seeds].sort((a, b) => {
    const rank = (s: SeedConnection) =>
      s.status === "FAULTED" ? 0 : s.status === "PENDING_CONFIG" ? 1 : 2
    const dr = rank(a) - rank(b)
    if (dr !== 0) return dr
    return (b.lastHealth?.at ?? "").localeCompare(a.lastHealth?.at ?? "")
  })

  const pageSize = input.pageSize ?? 20
  const page = Math.max(1, input.page)
  const total = seeds.length
  const start = (page - 1) * pageSize
  const pageSeeds = seeds.slice(start, start + pageSize)

  const items = pageSeeds.map((seed) => {
    const item = seedToListItem(seed)
    const { allowed, blockers } = roleActions(input.role, seed)
    return {
      ...item,
      allowedActions: allowed,
      actionBlockers: blockers,
    }
  })

  let emptyReason: ConnectionListView["emptyReason"]
  if (total === 0) {
    const anyInEnv =
      env === "ALL"
        ? allSeeds().length > 0
        : allSeeds().some((s) => s.environment === env)
    emptyReason = anyInEnv ? "FILTER_NO_RESULT" : "NO_CONNECTIONS"
  }

  return {
    metrics,
    items,
    total,
    page,
    pageSize,
    emptyReason,
    viewerRole: input.role,
    viewerRoleLabel: DEMO_ROLE_LABEL[input.role],
    hasModulePermission: true,
    hasDataScope: true,
    projectedAt: new Date().toISOString(),
    credentialOpaqueOptions: CREDENTIAL_OPAQUE_OPTIONS.map((o) => ({ ...o })),
  }
}

export async function fetchConnectionCenter(input: {
  connectionId: string
  role: DemoRole
}): Promise<ConnectionCenterView | null> {
  await mockDelay()
  const seed = findSeed(input.connectionId)
  if (!seed) return null
  return toCenter(seed, input.role)
}

function ensureOverlay(connectionId: string, seed: SeedConnection): SessionOverlay {
  let o = overlays.get(connectionId)
  if (!o) {
    o = { version: seed.version }
    overlays.set(connectionId, o)
  }
  return o
}

export async function createConnection(input: {
  connectionCode: string
  supplierId: string
  supplierName: string
  environment: SeedConnection["environment"]
  role: DemoRole
  idempotencyKey: string
}): Promise<FormalOutcome> {
  await mockDelay(120)
  if (input.role !== "admin") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权创建连接",
      message: "仅系统管理员可创建连接身份",
    }
  }
  const code = input.connectionCode.trim().toUpperCase()
  if (!code) {
    return {
      status: "failed",
      code: "CODE_REQUIRED",
      title: "连接代码必填",
      message: "请填写全局唯一的连接代码（环境不是唯一键组成部分）",
    }
  }
  if (allSeeds().some((s) => s.connectionCode === code)) {
    return {
      status: "failed",
      code: "CODE_DUPLICATE",
      title: "连接代码已存在",
      message: `连接代码 ${code} 全局唯一，不可与环境组合复用`,
    }
  }

  const connectionId = `conn_new_${Date.now().toString(36)}`
  const now = new Date().toISOString()
  const seed: SeedConnection = {
    connectionId,
    connectionCode: code,
    supplier: { id: input.supplierId, name: input.supplierName },
    environment: input.environment,
    status: "PENDING_CONFIG",
    businessOwner: { id: "u_cur", label: "当前用户" },
    technicalOwner: { id: "u_ops", label: "运维组" },
    version: "1",
    updatedAt: now,
    endpoint: { state: "MISSING" },
    credential: { state: "MISSING" },
    capabilities: [],
    healthRecords: [],
    catalog: {
      state: "NEVER",
      stateLabel: CATALOG_LABEL.NEVER,
    },
    relatedImpact: {
      activeOfferings: 0,
      activePublications: 0,
      openSupplierOrders: 0,
      activeSyncJobs: 0,
    },
    auditEvents: [
      {
        eventId: `ae_${connectionId}`,
        at: now,
        actor: "系统管理员",
        action: "CREATE_CONNECTION",
        summary: `创建连接身份 ${code}`,
        auditNo: `AUD-W20-${Date.now().toString().slice(-4)}`,
      },
    ],
    nextStep: "绑定地址/密钥引用 → 配置能力 → 健康检查",
  }
  createdConnections.unshift(seed)
  setIdempotencySucceeded(input.idempotencyKey, "CREATE_CONNECTION", {
    connectionId,
  })

  return {
    status: "succeeded",
    title: "连接身份已创建",
    message: `已创建 ${code}，状态为待配置。下一步完成技术引用与能力配置。`,
    reference: connectionId,
    connectionVersion: "1",
    facts: [
      { label: "连接代码", value: code },
      { label: "供应商", value: input.supplierName },
      { label: "环境", value: ENVIRONMENT_LABEL[input.environment] },
      { label: "状态", value: "待配置" },
    ],
  }
}

export async function bindCredentialReference(input: {
  connectionId: string
  opaqueReferenceId: string
  expectedVersion: string
  role: DemoRole
  idempotencyKey: string
  forceUnknown?: boolean
}): Promise<FormalOutcome> {
  await mockDelay(140)
  if (input.role !== "ops" && input.role !== "admin") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权轮换密钥引用",
      message: "仅研发运维或系统管理员可绑定密钥管理系统引用",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  if (seed.version !== input.expectedVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "配置已更新",
      message: "连接配置已被他人更新，请重新加载后基于最新版本提交",
    }
  }
  const opt = CREDENTIAL_OPAQUE_OPTIONS.find(
    (o) => o.referenceId === input.opaqueReferenceId
  )
  if (!opt) {
    return {
      status: "failed",
      code: "INVALID_REFERENCE",
      title: "无效引用",
      message: "只能选择系统提供的密钥引用，不能明文输入",
    }
  }

  if (input.forceUnknown) {
    setIdempotencyPending(input.idempotencyKey, "BIND_CREDENTIAL_REFERENCE")
    return {
      status: "unknown",
      title: "密钥引用绑定结果未知",
      message:
        "不得乐观切换引用显示。请按原任务号查询最终结果；失败时保留旧有效引用。",
      operationId: `op_cred_${Date.now()}`,
      idempotencyKey: input.idempotencyKey,
    }
  }

  const o = ensureOverlay(input.connectionId, seed)
  o.version = bumpVersion(seed.version)
  o.credential = {
    state: "BOUND",
    alias: opt.alias,
    version: opt.version,
  }
  const now = new Date().toISOString()
  o.auditEvents = [
    {
      eventId: `ae_cred_${Date.now()}`,
      at: now,
      actor: DEMO_ROLE_LABEL[input.role],
      action: "BIND_CREDENTIAL_REFERENCE",
      summary: `密钥引用已绑定 ${opt.alias}（${opt.version}）· 无正文`,
      auditNo: `AUD-W20-${Date.now().toString().slice(-4)}`,
    },
    ...(seed.auditEvents ?? []),
  ]
  o.nextStep = "引用已更新 · 建议执行健康检查验证"

  const result: FormalOutcome = {
    status: "succeeded",
    title: "密钥引用已绑定",
    message: `已绑定安全别名 ${opt.alias}（版本 ${opt.version}）。接口永不返回密钥正文。`,
    reference: opt.referenceId,
    connectionVersion: o.version,
    auditEventId: o.auditEvents[0]?.auditNo,
    facts: [
      { label: "引用状态", value: REFERENCE_STATE_LABEL.BOUND },
      { label: "安全别名", value: opt.alias },
      { label: "引用版本", value: opt.version },
      { label: "配置版本", value: o.version },
    ],
  }
  setIdempotencySucceeded(input.idempotencyKey, "BIND_CREDENTIAL_REFERENCE", result)
  return result
}

export async function confirmCapabilityRequirement(input: {
  connectionId: string
  capabilityCode: CapabilityCode
  requirement: "REQUIRED" | "NOT_REQUIRED"
  reasonCode: string
  expectedConnectionVersion: string
  expectedCapabilityVersion: string
  role: DemoRole
  operationId: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  await mockDelay(120)
  if (input.role !== "procurement") {
    return {
      status: "blocked",
      code: "ROLE_NOT_PROCUREMENT",
      title: "仅采购可确认业务能力需求",
      message:
        "确认业务能力需求是采购追加式业务确认，不改变能力启停，不创建任务",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  if (seed.version !== input.expectedConnectionVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "连接信息已更新",
      message: "请重新加载后基于最新连接版本确认",
    }
  }
  const cap = seed.capabilities.find(
    (c) => c.capabilityCode === input.capabilityCode
  )
  if (!cap) {
    return {
      status: "failed",
      code: "CAP_NOT_FOUND",
      title: "能力不存在",
      message: "该连接上无此能力记录",
    }
  }
  if (cap.version !== input.expectedCapabilityVersion) {
    return {
      status: "failed",
      code: "CAP_VERSION_CONFLICT",
      title: "能力信息已更新",
      message: "请重新加载后基于最新能力版本确认",
    }
  }

  const o = ensureOverlay(input.connectionId, seed)
  o.version = bumpVersion(seed.version)
  o.capabilities = seed.capabilities.map((c) =>
    c.capabilityCode === input.capabilityCode
      ? {
          ...c,
          businessRequirement: input.requirement,
          businessRequirementLabel:
            input.requirement === "REQUIRED"
              ? "采购确认需要"
              : "采购确认不需要",
          // status 不变
        }
      : c
  )
  const now = new Date().toISOString()
  const confirmationId = `ccr_${input.operationId}`
  o.auditEvents = [
    {
      eventId: `ae_ccr_${Date.now()}`,
      at: now,
      actor: "采购",
      action: "CONFIRM_CAPABILITY_REQUIREMENT",
      summary: `${CAPABILITY_LABEL[input.capabilityCode]} → ${
        input.requirement === "REQUIRED" ? "需要" : "不需要"
      }（能力状态未变）`,
      auditNo: `AUD-W20-${Date.now().toString().slice(-4)}`,
    },
    ...(seed.auditEvents ?? []),
  ]

  const result: FormalOutcome = {
    status: "succeeded",
    title: "业务能力需求已确认",
    message:
      "已追加采购业务确认与审计记录；能力启停状态未变更，未创建或完成任务。",
    reference: confirmationId,
    connectionVersion: o.version,
    auditEventId: o.auditEvents[0]?.auditNo,
    facts: [
      { label: "能力", value: CAPABILITY_LABEL[input.capabilityCode] },
      {
        label: "需求",
        value: input.requirement === "REQUIRED" ? "需要" : "不需要",
      },
      { label: "能力状态", value: cap.statusLabel },
      { label: "operationId", value: input.operationId },
    ],
  }
  setIdempotencySucceeded(
    input.idempotencyKey,
    "CONFIRM_CAPABILITY_REQUIREMENT",
    result
  )
  return result
}

export async function updateCapabilities(input: {
  connectionId: string
  changes: Array<{ code: CapabilityCode; enabled: boolean }>
  expectedConnectionVersion: string
  expectedCapabilityVersions: Record<string, string>
  reasonCode: string
  role: DemoRole
  operationId: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  await mockDelay(130)
  if (input.role !== "admin") {
    return {
      status: "blocked",
      code: "UPDATE_CAPABILITIES_FORBIDDEN",
      title: "无权配置能力",
      message:
        "仅系统管理员有 UPDATE_CAPABILITIES；与采购业务确认无共用写入口",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  if (seed.version !== input.expectedConnectionVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "连接信息已更新",
      message: "冲突不覆盖，请重读后提交",
    }
  }

  const o = ensureOverlay(input.connectionId, seed)
  o.version = bumpVersion(seed.version)
  const changeMap = new Map(input.changes.map((c) => [c.code, c.enabled]))
  o.capabilities = seed.capabilities.map((c) => {
    if (!changeMap.has(c.capabilityCode)) return c
    const expected = input.expectedCapabilityVersions[c.capabilityCode]
    if (expected && expected !== c.version) {
      return c
    }
    const enabled = changeMap.get(c.capabilityCode)!
    return {
      ...c,
      status: enabled ? ("ENABLED" as const) : ("DISABLED" as const),
      statusLabel: enabled ? "启用" : "停用",
      verification: "UNVERIFIED" as const,
      verificationLabel: "未验证",
      version: bumpVersion(c.version),
    }
  })
  const now = new Date().toISOString()
  o.auditEvents = [
    {
      eventId: `ae_cap_${Date.now()}`,
      at: now,
      actor: "系统管理员",
      action: "UPDATE_CAPABILITIES",
      summary: `能力配置变更 ${input.changes.length} 项 · 需重新验证`,
      auditNo: `AUD-W20-${Date.now().toString().slice(-4)}`,
    },
    ...(seed.auditEvents ?? []),
  ]
  o.nextStep = "能力已变更 · 标记为未验证 · 请执行健康检查"

  const result: FormalOutcome = {
    status: "succeeded",
    title: "能力配置已更新",
    message: "能力版本已更新并标记为未验证。不直接修改商品/订单/发布数据。",
    reference: input.operationId,
    connectionVersion: o.version,
    facts: input.changes.map((c) => ({
      label: CAPABILITY_LABEL[c.code],
      value: c.enabled ? "启用" : "停用",
    })),
  }
  setIdempotencySucceeded(input.idempotencyKey, "UPDATE_CAPABILITIES", result)
  return result
}

export async function runHealthCheck(input: {
  connectionId: string
  expectedVersion: string
  role: DemoRole
  idempotencyKey: string
  forceUnknown?: boolean
}): Promise<FormalOutcome> {
  await mockDelay(150)
  if (input.role !== "ops" && input.role !== "admin") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权执行健康检查",
      message: "健康检查由研发运维执行",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  if (
    seed.endpoint.state === "MISSING" ||
    seed.credential.state === "MISSING"
  ) {
    return {
      status: "blocked",
      code: "REFS_NOT_READY",
      title: "引用未就绪",
      message: "请先绑定地址与密钥引用",
    }
  }
  if (seed.lastHealth?.result === "UNKNOWN" && !input.forceUnknown) {
    return {
      status: "blocked",
      code: "PRIOR_UNKNOWN",
      title: "存在未知结果",
      message: "请先按任务号查询上一笔健康检查，勿盲目重复发起",
    }
  }

  const jobId = `job_h_${Date.now()}`
  const jobNo = `HLTH-${seed.connectionCode.replace("CONN-", "")}-${Date.now()
    .toString()
    .slice(-4)}`
  const now = new Date().toISOString()

  if (input.forceUnknown) {
    setIdempotencyPending(input.idempotencyKey, "RUN_HEALTH_CHECK")
    const o = ensureOverlay(input.connectionId, seed)
    o.lastHealth = {
      at: now,
      result: "UNKNOWN",
      resultLabel: HEALTH_LABEL.UNKNOWN,
      errorClass: "RESULT_UNKNOWN",
      errorSummary: "结果未知 · 按任务号查询",
    }
    return {
      status: "unknown",
      title: "健康检查结果未知",
      message: `任务 ${jobNo} 结果不确定。不得按成功/失败处理；用原任务号查询。`,
      operationId: jobId,
      idempotencyKey: input.idempotencyKey,
    }
  }

  // simulate success unless auth-failed credential rotation due on sf
  const authRisk =
    seed.credential.state === "ROTATION_DUE" &&
    seed.lastHealth?.result === "AUTH_FAILED"
  const resultHealth: HealthResult = authRisk ? "AUTH_FAILED" : "SUCCESS"

  activeJobs.set(jobId, {
    jobId,
    jobNo,
    kind: "HEALTH",
    connectionId: input.connectionId,
    status: resultHealth === "SUCCESS" ? "succeeded" : "failed",
    total: seed.capabilities.filter((c) => c.status === "ENABLED").length || 1,
    completed:
      seed.capabilities.filter((c) => c.status === "ENABLED").length || 1,
    succeeded: resultHealth === "SUCCESS" ? 1 : 0,
    failed: resultHealth === "SUCCESS" ? 0 : 1,
    createdAt: now,
  })

  const o = ensureOverlay(input.connectionId, seed)
  o.lastHealth = {
    at: now,
    result: resultHealth,
    resultLabel: HEALTH_LABEL[resultHealth],
    latencyMs: 160,
    traceId: `tr_${jobId}`,
    autoRetryStopped: resultHealth === "AUTH_FAILED",
    errorClass:
      resultHealth === "AUTH_FAILED" ? "AUTH_SIGNATURE_FAILURE" : undefined,
    errorSummary:
      resultHealth === "AUTH_FAILED"
        ? "鉴权/签名失败。自动重试已停止。"
        : undefined,
  }
  o.healthRecords = [
    {
      recordId: `hr_${jobId}`,
      at: now,
      checkType:
        seed.environment === "PRODUCTION"
          ? "生产全能力检查（不创建业务订单）"
          : "全能力健康检查",
      result: resultHealth,
      resultLabel: HEALTH_LABEL[resultHealth],
      resultTone: HEALTH_TONE[resultHealth],
      latencyMs: 160,
      autoRetryStopped: resultHealth === "AUTH_FAILED",
      errorClass: o.lastHealth.errorClass,
      errorSummary: o.lastHealth.errorSummary,
      traceId: `tr_${jobId}`,
      jobId,
      jobNo,
    },
    ...(seed.healthRecords ?? []),
  ]
  if (resultHealth === "AUTH_FAILED") {
    o.status = "FAULTED"
    o.alerts = [
      {
        id: `al_auth_${jobId}`,
        severity: "destructive",
        title: "鉴权/签名失败 · 自动重试已停止",
        description:
          "高风险故障。请运维轮换密钥引用并复查适配器；本页不展示密钥正文。",
      },
    ]
    o.nextStep = "轮换密钥引用并重新健康检查"
  } else {
    o.nextStep = "健康检查通过"
    if (seed.status === "FAULTED") {
      // keep faulted until admin enables — don't auto-enable
    }
  }

  return {
    status: "processing",
    title: "健康检查任务已创建",
    message: `已创建后台任务 ${jobNo}。请求返回不等于检查完成；请查看任务进度与固定结果。${
      seed.environment === "PRODUCTION"
        ? " 生产检查不会创建业务订单。"
        : ""
    }`,
    jobId,
    jobNo,
  }
}

export async function startCatalogSync(input: {
  connectionId: string
  role: DemoRole
  idempotencyKey: string
}): Promise<FormalOutcome> {
  await mockDelay(140)
  if (input.role === "procurement") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权触发目录同步",
      message: "目录同步由运维或管理员在能力启用且健康时触发",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  const catalogCap = seed.capabilities.find(
    (c) => c.capabilityCode === "CATALOG" && c.status === "ENABLED"
  )
  if (!catalogCap) {
    return {
      status: "blocked",
      code: "CAPABILITY",
      title: "目录能力未启用",
      message: "CATALOG 能力未启用，无法触发同步",
    }
  }
  if (seed.lastHealth?.result === "AUTH_FAILED") {
    return {
      status: "blocked",
      code: "AUTH_FAILED",
      title: "鉴权失败",
      message: "鉴权失败期间禁止目录同步",
    }
  }

  const jobId = `job_cat_${Date.now()}`
  const jobNo = `CAT-${seed.connectionCode.replace("CONN-", "")}-${Date.now()
    .toString()
    .slice(-4)}`
  const now = new Date().toISOString()
  activeJobs.set(jobId, {
    jobId,
    jobNo,
    kind: "CATALOG",
    connectionId: input.connectionId,
    status: "running",
    total: 1000,
    completed: 120,
    succeeded: 118,
    failed: 2,
    createdAt: now,
  })

  const o = ensureOverlay(input.connectionId, seed)
  o.catalog = {
    state: "RUNNING",
    stateLabel: CATALOG_LABEL.RUNNING,
    lastSuccessfulAt: seed.catalog.lastSuccessfulAt,
    activeJobId: jobId,
    activeJobNo: jobNo,
    progress: {
      status: "running",
      total: 1000,
      completed: 120,
      succeeded: 118,
      failed: 2,
    },
  }
  o.nextStep = `目录同步进行中 · ${jobNo}`

  setIdempotencySucceeded(input.idempotencyKey, "START_CATALOG_SYNC", {
    jobId,
    jobNo,
  })

  return {
    status: "processing",
    title: "目录同步任务已创建",
    message: `已创建目录同步任务 ${jobNo}。可在本页查看进度或进入供应商商品库。`,
    jobId,
    jobNo,
  }
}

export async function disableConnection(input: {
  connectionId: string
  expectedVersion: string
  role: DemoRole
  reasonCode: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  await mockDelay(130)
  if (input.role !== "admin") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权停用连接",
      message: "停用连接仅系统管理员可执行",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  if (seed.version !== input.expectedVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "数据已更新",
      message: "请重新加载后提交",
    }
  }

  const o = ensureOverlay(input.connectionId, seed)
  o.status = "DISABLED"
  o.version = bumpVersion(seed.version)
  const now = new Date().toISOString()
  o.auditEvents = [
    {
      eventId: `ae_dis_${Date.now()}`,
      at: now,
      actor: "系统管理员",
      action: "DISABLE",
      summary: "连接已停用；不删除历史版本与业务记录",
      auditNo: `AUD-W20-${Date.now().toString().slice(-4)}`,
    },
    ...(seed.auditEvents ?? []),
  ]
  o.nextStep = "已停用 · 历史保留 · 不删除版本与业务记录"

  const result: FormalOutcome = {
    status: "succeeded",
    title: "连接已停用",
    message:
      "连接状态变为停用。不删除连接、版本和历史业务；发布/订单/同步数据保留。",
    reference: input.connectionId,
    connectionVersion: o.version,
    facts: [
      {
        label: "受影响发布",
        value: String(seed.relatedImpact.activePublications),
      },
      {
        label: "待处理供应商订单",
        value: String(seed.relatedImpact.openSupplierOrders),
      },
      {
        label: "进行中同步",
        value: String(seed.relatedImpact.activeSyncJobs),
      },
      { label: "历史", value: "保留" },
    ],
  }
  setIdempotencySucceeded(input.idempotencyKey, "DISABLE", result)
  return result
}

export async function enableConnection(input: {
  connectionId: string
  expectedVersion: string
  role: DemoRole
  idempotencyKey: string
}): Promise<FormalOutcome> {
  await mockDelay(130)
  if (input.role !== "admin") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权启用连接",
      message: "启用由系统管理员执行",
    }
  }
  const seed = findSeed(input.connectionId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "连接不存在",
      message: "未找到连接",
    }
  }
  if (
    seed.endpoint.state === "MISSING" ||
    seed.credential.state === "MISSING"
  ) {
    return {
      status: "blocked",
      code: "PRECONDITIONS",
      title: "前置未满足",
      message: "地址与密钥引用需就绪",
    }
  }

  const o = ensureOverlay(input.connectionId, seed)
  o.status = "ENABLED"
  o.version = bumpVersion(seed.version)
  o.nextStep = "已启用"
  o.alerts = []

  return {
    status: "succeeded",
    title: "连接已启用",
    message: "状态变为启用。不直接修改供应商商品、供给或历史订单。",
    reference: input.connectionId,
    connectionVersion: o.version,
  }
}

export async function queryFormalByIdempotency(
  idempotencyKey: string
): Promise<FormalOutcome | null> {
  await mockDelay(60)
  const entry = getIdempotencyEntry(idempotencyKey)
  if (!entry) return null
  if (entry.state === "pending") {
    return {
      status: "unknown",
      title: "结果仍未知",
      message: "系统尚未给出最终结论，请稍后用原任务号再查。",
      operationId: idempotencyKey,
      idempotencyKey,
    }
  }
  if (entry.state === "succeeded") {
    const payload = entry.payload as FormalOutcome
    if (payload && typeof payload === "object" && "status" in payload) {
      return payload
    }
    return {
      status: "succeeded",
      title: resultText.operationSucceeded,
      message: "按原任务号查询到成功结果",
      reference: idempotencyKey,
    }
  }
  return {
    status: "failed",
    code: "FAILED",
    title: "操作失败",
    message: entry.error ?? "按原任务号查询到失败结果",
  }
}

export function getActiveJob(jobId: string) {
  return activeJobs.get(jobId) ?? null
}

export function referenceStateLabel(state: ReferenceState): string {
  return REFERENCE_STATE_LABEL[state]
}

// silence unused workspace marker for future session scoping
void WORKSPACE
