/**
 * W20 · API 供应商连接 · 真实 HTTP 适配层。
 * 保持对外导出签名稳定；后端 Page/DTO 在此映射为 feature 视图类型。
 */

import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import type {
  CapabilityCode,
  ConnectionCenterView,
  ConnectionEnvironment,
  ConnectionListItem,
  ConnectionListView,
  ConnectionStatus,
  FormalOutcome,
  HealthResult,
  ReferenceState,
} from "@/features/supplier-api-connections/types"
import {
  CAPABILITY_LABEL,
  CATALOG_LABEL,
  ENVIRONMENT_LABEL,
  HEALTH_LABEL,
  HEALTH_TONE,
  REFERENCE_STATE_LABEL,
  STATUS_LABEL,
  STATUS_TONE,
} from "@/features/supplier-api-connections/types"

// ─── Backend wire types ───────────────────────────────────────────────────────

type BackendConnection = {
  id: string
  supplier_id: string
  connection_code: string
  environment: "production" | "testing" | string
  status: "active" | "disabled" | "fault" | string
  rate_limit_policy?: { max_requests: number; window_secs: number } | null
  last_health_at?: number | null
  last_health_result?: "healthy" | "failed" | string | null
  version: number
  created_at: number
}

type BackendCapability = {
  id: string
  connection_id: string
  capability_code: string
  status: "active" | "disabled" | string
  version: number
  created_at: number
}

type BackendConnectionDetail = BackendConnection & {
  capabilities: BackendCapability[]
}

type BackendHealthCheck = {
  checked_at: number
  result: "healthy" | "failed" | string
  inbox_message_id: string
  error_task_id?: string | null
  version: number
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function secsToIso(secs?: number | null): string | undefined {
  if (secs == null || secs <= 0) return undefined
  return new Date(secs * 1000).toISOString()
}

function mapEnvironment(raw: string): ConnectionEnvironment {
  if (raw === "production") return "PRODUCTION"
  if (raw === "testing") return "STAGING"
  const upper = raw.toUpperCase()
  if (upper === "DEVELOPMENT" || upper === "STAGING" || upper === "PRODUCTION") {
    return upper as ConnectionEnvironment
  }
  return "STAGING"
}

function toBackendEnvironment(
  env: ConnectionEnvironment
): "production" | "testing" {
  return env === "PRODUCTION" ? "production" : "testing"
}

function mapStatus(raw: string): ConnectionStatus {
  switch (raw) {
    case "active":
      return "ENABLED"
    case "disabled":
      return "DISABLED"
    case "fault":
      return "FAULTED"
    default:
      return "PENDING_CONFIG"
  }
}

function toBackendStatus(
  status: ConnectionStatus
): "active" | "disabled" | "fault" {
  switch (status) {
    case "ENABLED":
      return "active"
    case "DISABLED":
      return "disabled"
    case "FAULTED":
      return "fault"
    default:
      return "active"
  }
}

function mapHealth(raw?: string | null): HealthResult {
  if (!raw) return "UNCHECKED"
  if (raw === "healthy") return "SUCCESS"
  if (raw === "failed") return "FAILED"
  return "UNCHECKED"
}

function mapCapabilityCode(raw: string): CapabilityCode {
  const table: Record<string, CapabilityCode> = {
    product: "CATALOG",
    price: "PRICE",
    stock: "STOCK",
    order: "ORDER",
    query: "QUERY",
    cancel: "CANCEL",
    refund: "REFUND",
    logistics: "LOGISTICS",
    callback: "CALLBACK",
    settlement: "SETTLEMENT",
    catalog: "CATALOG",
  }
  return table[raw.toLowerCase()] ?? "CATALOG"
}

function toBackendCapabilityCode(code: CapabilityCode): string {
  const table: Record<CapabilityCode, string> = {
    CATALOG: "product",
    PRICE: "price",
    STOCK: "stock",
    ORDER: "order",
    QUERY: "query",
    CANCEL: "cancel",
    REFUND: "refund",
    LOGISTICS: "logistics",
    CALLBACK: "callback",
    SETTLEMENT: "settlement",
  }
  return table[code]
}

function mapCapability(c: BackendCapability) {
  const code = mapCapabilityCode(c.capability_code)
  const enabled = c.status === "active"
  return {
    capabilityCode: code,
    capabilityLabel: CAPABILITY_LABEL[code],
    status: (enabled ? "ENABLED" : "DISABLED") as "ENABLED" | "DISABLED",
    statusLabel: enabled ? "启用" : "停用",
    verification: "UNVERIFIED" as const,
    verificationLabel: "未验证",
    businessRequirement: "UNCONFIRMED" as const,
    businessRequirementLabel: "未确认",
    version: String(c.version),
    productLevelNote:
      "连接级能力声明 ≠ 每个商品可用；商品/供给/发布级能力见供应商商品库 / 商品发布",
  }
}

function toListItem(
  conn: BackendConnection,
  caps: BackendCapability[],
  supplierName?: string
): ConnectionListItem {
  const status = mapStatus(conn.status)
  const env = mapEnvironment(conn.environment)
  const health = mapHealth(conn.last_health_result)
  const enabledCaps = caps.filter((c) => c.status === "active")
  const capabilitySummary =
    enabledCaps.length === 0
      ? "未配置能力"
      : enabledCaps
          .map((c) => CAPABILITY_LABEL[mapCapabilityCode(c.capability_code)])
          .join("、")

  return {
    connectionId: conn.id,
    connectionCode: conn.connection_code,
    supplier: { id: conn.supplier_id, name: supplierName ?? conn.supplier_id },
    environment: env,
    environmentLabel: ENVIRONMENT_LABEL[env],
    status,
    statusLabel: STATUS_LABEL[status],
    statusTone: STATUS_TONE[status],
    capabilitySummary,
    healthResult: health,
    healthLabel: HEALTH_LABEL[health],
    healthTone: HEALTH_TONE[health],
    lastHealthAt: secsToIso(conn.last_health_at),
    catalogState: "NEVER",
    catalogLabel: CATALOG_LABEL.NEVER,
    nextStep: status === "ENABLED" ? "连接已启用" : "完成配置与健康检查",
    allowedActions: [],
    actionBlockers: [],
  }
}

function toCenter(
  detail: BackendConnectionDetail,
  supplierName?: string
): ConnectionCenterView {
  const status = mapStatus(detail.status)
  const env = mapEnvironment(detail.environment)
  const health = mapHealth(detail.last_health_result)
  const caps = detail.capabilities ?? []
  const healthAt = secsToIso(detail.last_health_at)

  // 密钥引用永不回显（后端契约）；前端仅展示「未绑定」占位。
  const missingRef = {
    state: "MISSING" as ReferenceState,
    visible: true,
  }

  return {
    connectionId: detail.id,
    connectionCode: detail.connection_code,
    supplier: {
      id: detail.supplier_id,
      name: supplierName ?? detail.supplier_id,
    },
    environment: env,
    environmentLabel: ENVIRONMENT_LABEL[env],
    status,
    statusLabel: STATUS_LABEL[status],
    statusTone: STATUS_TONE[status],
    adapter: { code: "default", version: "1", visible: true },
    version: String(detail.version),
    updatedAt: secsToIso(detail.created_at) ?? new Date(0).toISOString(),
    safeReferences: {
      endpoint: missingRef,
      credential: missingRef,
    },
    capabilities: caps.map(mapCapability),
    lastHealth: healthAt
      ? {
          at: healthAt,
          result: health,
          resultLabel: HEALTH_LABEL[health],
        }
      : undefined,
    healthRecords: healthAt
      ? [
          {
            recordId: `hr_${detail.id}_${detail.last_health_at}`,
            at: healthAt,
            checkType: "健康检查",
            result: health,
            resultLabel: HEALTH_LABEL[health],
            resultTone: HEALTH_TONE[health],
          },
        ]
      : [],
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
    auditEvents: [],
    allowedActions: [],
    actionBlockers: [],
    alerts: [],
    nextStep:
      status === "ENABLED"
        ? "连接已启用"
        : "绑定地址/密钥引用 → 配置能力 → 健康检查",
  }
}

async function resolveSupplierName(supplierId: string): Promise<string> {
  try {
    const row = await apiGet<{ id: string; name?: string; supplier_name?: string }>(
      `/admin/suppliers/${encodeURIComponent(supplierId)}`
    )
    return row.name ?? row.supplier_name ?? supplierId
  } catch {
    return supplierId
  }
}

// ─── Public API (stable signatures) ───────────────────────────────────────────

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
}

export async function fetchConnectionList(
  input: ListQueryInput
): Promise<ConnectionListView> {
  const page = Math.max(1, input.page)
  const pageSize = input.pageSize ?? 20
  const env = input.environment.toUpperCase()

  const query: Record<string, unknown> = {
    page,
    page_size: pageSize,
    sort_by: "updated_at",
    sort_dir: "desc",
  }
  if (input.supplierId) query.supplier_id = input.supplierId
  if (input.q?.trim()) query.connection_code = input.q.trim()
  if (env !== "ALL") {
    query.environment =
      env === "PRODUCTION" ? "production" : env === "DEVELOPMENT" ? "testing" : "testing"
  }
  if (input.status) {
    const first = input.status.split(",")[0]?.trim().toUpperCase()
    if (first === "ENABLED") query.status = "active"
    else if (first === "DISABLED") query.status = "disabled"
    else if (first === "FAULTED") query.status = "fault"
  }

  const pageResult = await apiGet<Page<BackendConnection>>(
    "/admin/supplier-api-connections",
    query
  )

  // 能力列表：按页批量拉取（后端无连接内嵌能力的列表投影）
  const capPage = await apiGet<Page<BackendCapability>>(
    "/admin/supplier-api-capabilities",
    { page: 1, page_size: 100 }
  ).catch(() => ({ items: [] as BackendCapability[], total: 0, page: 1, page_size: 100 }))

  const capsByConn = new Map<string, BackendCapability[]>()
  for (const cap of capPage.items) {
    const list = capsByConn.get(cap.connection_id) ?? []
    list.push(cap)
    capsByConn.set(cap.connection_id, list)
  }

  const supplierNames = new Map<string, string>()
  await Promise.all(
    pageResult.items.map(async (c) => {
      if (!supplierNames.has(c.supplier_id)) {
        supplierNames.set(c.supplier_id, await resolveSupplierName(c.supplier_id))
      }
    })
  )

  let items = pageResult.items.map((c) =>
    toListItem(c, capsByConn.get(c.id) ?? [], supplierNames.get(c.supplier_id))
  )

  // 客户端补筛：后端未提供 health / capability / catalog 筛选
  if (input.health) {
    const hs = input.health.split(",").map((h) => h.trim().toUpperCase())
    items = items.filter((i) => hs.includes(i.healthResult))
  }
  if (input.capability) {
    const code = input.capability.toUpperCase()
    items = items.filter((i) =>
      i.capabilitySummary.includes(
        CAPABILITY_LABEL[code as CapabilityCode] ?? code
      )
    )
  }

  // 指标：仅基于当前页（后端无汇总端点）— 登记为 backend_gap
  const metrics = {
    enabled: items.filter((i) => i.status === "ENABLED").length,
    faulted: items.filter((i) => i.status === "FAULTED").length,
    pendingConfig: items.filter((i) => i.status === "PENDING_CONFIG").length,
    healthAbnormal: items.filter((i) =>
      ["FAILED", "AUTH_FAILED", "PARTIAL", "UNKNOWN"].includes(i.healthResult)
    ).length,
    catalogStale: 0,
  }

  const emptyReason: ConnectionListView["emptyReason"] =
    pageResult.total === 0
      ? items.length === 0
        ? "NO_CONNECTIONS"
        : "FILTER_NO_RESULT"
      : undefined

  return {
    metrics,
    items,
    total: pageResult.total,
    page: pageResult.page,
    pageSize: pageResult.page_size,
    emptyReason,
    hasModulePermission: true,
    hasDataScope: true,
    projectedAt: secsToIso(
      Math.max(0, ...pageResult.items.map((c) => c.created_at))
    ) ?? new Date(0).toISOString(),
    credentialOpaqueOptions: [],
    endpointOpaqueOptions: [],
  }
}

export async function fetchConnectionCenter(input: {
  connectionId: string
}): Promise<ConnectionCenterView | null> {
  try {
    const detail = await apiGet<BackendConnectionDetail>(
      `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`
    )
    const supplierName = await resolveSupplierName(detail.supplier_id)
    return toCenter(detail, supplierName)
  } catch (err) {
    const e = err as { kind?: string; status?: number }
    if (e?.kind === "Http" && e.status === 404) return null
    throw err
  }
}

export async function createConnection(input: {
  connectionCode: string
  supplierId: string
  supplierName: string
  environment: ConnectionEnvironment
  idempotencyKey: string
}): Promise<FormalOutcome> {
  const code = input.connectionCode.trim().toUpperCase()
  if (!code) {
    return {
      status: "failed",
      code: "CODE_REQUIRED",
      title: "连接代码必填",
      message: "请填写全局唯一的连接代码，不可与环境组合复用",
    }
  }

  // 后端 Create 要求 endpoint_reference 非空；占位引用，运维后续 PUT 绑定
  const created = await apiPost<BackendConnectionDetail>(
    "/admin/supplier-api-connections",
    {
      supplier_id: input.supplierId,
      connection_code: code,
      environment: toBackendEnvironment(input.environment),
      endpoint_reference: `pending:${input.idempotencyKey}`,
      credential_reference: null,
      rate_limit_policy: null,
      status: "disabled",
      capabilities: [],
    }
  )

  return {
    status: "succeeded",
    title: "连接身份已创建",
    message: `已创建 ${code}。下一步完成技术引用与能力配置。`,
    reference: code,
    connectionId: created.id,
    connectionVersion: String(created.version),
    facts: [
      { label: "连接代码", value: code },
      { label: "供应商", value: input.supplierName },
      { label: "环境", value: ENVIRONMENT_LABEL[input.environment] },
      { label: "状态", value: STATUS_LABEL[mapStatus(created.status)] },
    ],
  }
}

export async function bindCredentialReference(input: {
  connectionId: string
  opaqueReferenceId: string
  expectedVersion: string
  idempotencyKey: string
  forceUnknown?: boolean
}): Promise<FormalOutcome> {
  if (input.forceUnknown) {
    return {
      status: "unknown",
      title: "密钥引用绑定结果未知",
      message:
        "不得乐观切换引用显示。请按原任务号查询最终结果；失败时保留旧有效引用。",
      operationId: `op_cred_${input.idempotencyKey}`,
      idempotencyKey: input.idempotencyKey,
    }
  }

  const updated = await apiPut<BackendConnection>(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`,
    {
      version: Number(input.expectedVersion) || 1,
      credential_reference: input.opaqueReferenceId,
    }
  )

  return {
    status: "succeeded",
    title: "密钥引用已绑定",
    message: "已绑定密钥管理系统引用。接口永不返回密钥正文。",
    reference: updated.connection_code,
    connectionVersion: String(updated.version),
    facts: [
      { label: "引用状态", value: REFERENCE_STATE_LABEL.BOUND },
      { label: "配置版本", value: String(updated.version) },
    ],
  }
}

export async function bindEndpointReference(input: {
  connectionId: string
  opaqueReferenceId: string
  expectedVersion: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  const updated = await apiPut<BackendConnection>(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`,
    {
      version: Number(input.expectedVersion) || 1,
      endpoint_reference: input.opaqueReferenceId,
    }
  )

  return {
    status: "succeeded",
    title: "地址引用已绑定",
    message: "已绑定地址配置引用。",
    reference: updated.connection_code,
    connectionVersion: String(updated.version),
    facts: [
      { label: "引用状态", value: REFERENCE_STATE_LABEL.BOUND },
      { label: "配置版本", value: String(updated.version) },
    ],
  }
}

export async function confirmCapabilityRequirement(input: {
  connectionId: string
  capabilityCode: CapabilityCode
  requirement: "REQUIRED" | "NOT_REQUIRED"
  reasonCode: string
  expectedConnectionVersion: string
  expectedCapabilityVersion: string
  operationId: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  // 后端无独立「业务能力需求确认」端点 — 登记 backend_gap；
  // 不调用 UPDATE_CAPABILITIES（会改启停），仅返回成功占位并提示审计缺口。
  return {
    status: "blocked",
    code: "BACKEND_GAP",
    title: "业务能力需求确认尚未接入",
    message:
      "后端暂无采购业务能力需求确认接口；能力启停请由管理员通过能力配置接口处理。",
    reference: input.operationId,
  }
}

export async function updateCapabilities(input: {
  connectionId: string
  changes: Array<{ code: CapabilityCode; enabled: boolean }>
  expectedConnectionVersion: string
  expectedCapabilityVersions: Record<string, string>
  reasonCode: string
  operationId: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  // 后端为整表替换；先读详情再合并
  const detail = await apiGet<BackendConnectionDetail>(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`
  )
  const changeMap = new Map(input.changes.map((c) => [c.code, c.enabled]))
  const existing = (detail.capabilities ?? []).map((c) => {
    const code = mapCapabilityCode(c.capability_code)
    if (!changeMap.has(code)) {
      return {
        capability_code: c.capability_code,
        status: c.status,
        constraint_snapshot: null as string | null,
      }
    }
    const enabled = changeMap.get(code)!
    return {
      capability_code: toBackendCapabilityCode(code),
      status: enabled ? "active" : "disabled",
      constraint_snapshot: null as string | null,
    }
  })
  // 新增能力
  for (const ch of input.changes) {
    if (!existing.some((e) => mapCapabilityCode(e.capability_code) === ch.code)) {
      existing.push({
        capability_code: toBackendCapabilityCode(ch.code),
        status: ch.enabled ? "active" : "disabled",
        constraint_snapshot: null,
      })
    }
  }

  await apiPut(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}/capabilities`,
    {
      expected_connection_version:
        Number(input.expectedConnectionVersion) || detail.version,
      capabilities: existing,
    }
  )

  return {
    status: "succeeded",
    title: "能力配置已更新",
    message: "能力版本已更新。不直接修改商品/订单/发布数据。",
    reference: input.operationId,
    connectionVersion: String(
      (Number(input.expectedConnectionVersion) || detail.version) + 1
    ),
    facts: input.changes.map((c) => ({
      label: CAPABILITY_LABEL[c.code],
      value: c.enabled ? "启用" : "停用",
    })),
  }
}

export async function runHealthCheck(input: {
  connectionId: string
  expectedVersion: string
  idempotencyKey: string
  forceUnknown?: boolean
}): Promise<FormalOutcome> {
  if (input.forceUnknown) {
    return {
      status: "unknown",
      title: "健康检查结果未知",
      message: "结果不确定。不得按成功/失败处理；用原任务号查询。",
      operationId: `op_health_${input.idempotencyKey}`,
      idempotencyKey: input.idempotencyKey,
    }
  }

  const result = await apiPost<BackendHealthCheck>(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}/health-check`,
    { idempotency_key: input.idempotencyKey }
  )

  const health = mapHealth(result.result)
  return {
    status: "succeeded",
    title: "健康检查已完成",
    message:
      health === "SUCCESS"
        ? "健康检查通过。"
        : "健康检查失败；请查看错误任务与密钥引用。",
    reference: result.inbox_message_id,
    connectionVersion: String(result.version),
    facts: [
      { label: "结果", value: HEALTH_LABEL[health] },
      { label: "消息信封", value: result.inbox_message_id },
      ...(result.error_task_id
        ? [{ label: "错误任务", value: result.error_task_id }]
        : []),
    ],
  }
}

export async function startCatalogSync(input: {
  connectionId: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  // 后端本域无目录同步触发端点（同步由 supplier_catalog intake 承接）
  return {
    status: "blocked",
    code: "BACKEND_GAP",
    title: "目录同步尚未接入本页",
    message:
      "供应商目录同步不在 supplier-api-connections 域；请到供应商商品库或集成任务中处理。",
    reference: input.connectionId,
  }
}

export async function disableConnection(input: {
  connectionId: string
  expectedVersion: string
  reasonCode: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  const updated = await apiPut<BackendConnection>(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`,
    {
      version: Number(input.expectedVersion) || 1,
      status: "disabled",
    }
  )

  return {
    status: "succeeded",
    title: "连接已停用",
    message:
      "连接状态变为停用。不删除连接、版本和历史业务；发布/订单/同步数据保留。",
    reference: updated.connection_code,
    connectionVersion: String(updated.version),
  }
}

export async function enableConnection(input: {
  connectionId: string
  expectedVersion: string
  idempotencyKey: string
}): Promise<FormalOutcome> {
  const updated = await apiPut<BackendConnection>(
    `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`,
    {
      version: Number(input.expectedVersion) || 1,
      status: toBackendStatus("ENABLED"),
    }
  )

  return {
    status: "succeeded",
    title: "连接已启用",
    message: "状态变为启用。不直接修改供应商商品、供给或历史订单。",
    reference: updated.connection_code,
    connectionVersion: String(updated.version),
  }
}

export function getActiveJob(jobId: string) {
  void jobId
  return null
}

export function referenceStateLabel(state: ReferenceState): string {
  return REFERENCE_STATE_LABEL[state]
}
