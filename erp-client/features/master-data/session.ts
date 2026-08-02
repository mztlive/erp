/**
 * W14 session-only mutable state for create / revise / disable demos.
 */

import {
  MASTER_DATA_CENTER_SEEDS,
  MASTER_DATA_LIST_SEEDS,
  WAREHOUSE_WRITE_CODE,
  WAREHOUSE_WRITE_MESSAGE,
  computeMetrics,
  resourceLabel,
} from "@/features/master-data/data"
import type {
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataListResult,
  MasterDataMutationResult,
  MasterDataResource,
  SelectorCandidate,
  SelectorQueryResult,
  SelectorQueryScene,
} from "@/features/master-data/types"

const listOverlays = new Map<string, MasterDataListItem>()
const centerOverlays = new Map<string, MasterDataCenterView>()
const createdIdsByResource = new Map<MasterDataResource, string[]>()
const idempotencyResults = new Map<string, MasterDataMutationResult>()

const ACTOR = "当前用户"

function listKey(resource: MasterDataResource, stableId: string) {
  return `${resource}:${stableId}`
}

function cloneListSeeds(resource: MasterDataResource): MasterDataListItem[] {
  const base = MASTER_DATA_LIST_SEEDS[resource].map((row) => {
    const overlay = listOverlays.get(listKey(resource, row.stableId))
    return overlay ?? row
  })
  const created = (createdIdsByResource.get(resource) ?? [])
    .map((id) => listOverlays.get(listKey(resource, id)))
    .filter((x): x is MasterDataListItem => Boolean(x))
  return [...created, ...base]
}

export function listW14Rows(resource: MasterDataResource): MasterDataListItem[] {
  return cloneListSeeds(resource)
}

export function getW14Center(
  resource: MasterDataResource,
  stableId: string
): MasterDataCenterView | null {
  const overlay = centerOverlays.get(listKey(resource, stableId))
  if (overlay) return overlay
  const seed = MASTER_DATA_CENTER_SEEDS[stableId]
  if (!seed || seed.resource !== resource) return null
  return seed
}

export function buildW14ListResult(
  resource: MasterDataResource
): MasterDataListResult {
  const rows = listW14Rows(resource)
  const now = new Date().toISOString()
  return {
    resource,
    rows,
    totalCount: rows.length,
    permissionVersion: "pv-w14-demo-1",
    effectiveAsOf: now,
    eligibilityAsOf: now,
    queriedAt: now,
    metrics: [...computeMetrics(rows)],
    permissionDemo: {
      hasModuleAccess: true,
      resourceAccess: {
        "sellable-items": true,
        products: true,
        "voucher-categories": true,
        suppliers: true,
        warehouses: true,
      },
      canExport: true,
      roleLabel: "采购",
      canRevealSensitive: true,
    },
  }
}

function nextStableNo(resource: MasterDataResource, index: number): string {
  const prefix: Record<MasterDataResource, string> = {
    "sellable-items": "SI-2026",
    products: "SKU-NEW",
    "voucher-categories": "VC-NEW",
    suppliers: "SUP-2026",
    warehouses: "WH-NEW",
  }
  return `${prefix[resource]}-${String(9000 + index).padStart(4, "0")}`
}

function rejectWarehouseWrite(): MasterDataMutationResult {
  return {
    outcome: "blocked",
    code: WAREHOUSE_WRITE_CODE,
    message: WAREHOUSE_WRITE_MESSAGE,
    detail: "服务端拒绝写入；仓储与系统管理员均不可作为临时写入人。",
  }
}

export function createW14Object(
  input: CreateMasterDataInput
): MasterDataMutationResult {
  const cached = idempotencyResults.get(input.idempotencyKey)
  if (cached) return cached

  if (input.resource === "warehouses") {
    const blocked = rejectWarehouseWrite()
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "overlap") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "EFFECTIVE_RANGE_OVERLAP",
      message: "生效区间与已有修订重叠，服务端拒绝创建。",
      detail: `与当前 v1 区间 ${input.effectiveFrom} 起冲突，请调整 effectiveFrom/To。`,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "sku_signature" && input.resource === "products") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "SPEC_SIGNATURE_IMMUTABLE",
      message: "规格身份变化必须新建 SKU，不能在创建路径伪造签名变更。",
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  const seq = (createdIdsByResource.get(input.resource)?.length ?? 0) + 1
  const stableId = `${input.resource.replace(/-/g, "_")}_new_${seq}`
  const stableNo = nextStableNo(input.resource, seq)
  const revisionId = `${stableId}_r1`
  const recordedAt = new Date().toISOString()
  const effectiveFrom = input.effectiveFrom

  const listItem: MasterDataListItem = {
    objectType: input.resource,
    stableId,
    stableNo,
    name: input.name.trim(),
    lifecycleStatus: "ENABLED",
    lifecycleStatusLabel: "当前启用",
    lifecycleTone: "success",
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前",
    currentRevisionId: revisionId,
    displayedRevisionId: revisionId,
    revisionNo: 1,
    effectiveFrom,
    effectiveTo: input.effectiveTo,
    keyFacts: [
      { label: "资源", value: resourceLabel(input.resource) },
      { label: "说明", value: "会话新建" },
    ],
    selectorEligibility: [
      {
        context: "default",
        contextLabel: "业务选择器",
        eligible: true,
        blockerCodes: [],
      },
    ],
    allowedActions: ["VIEW", "CREATE_REVISION", "DISABLE", "EXPORT_ROW"],
    actionBlockers: [],
    lockVersion: 1,
    ownerName: ACTOR,
    metricTags: ["enabled"],
  }

  const center: MasterDataCenterView = {
    resource: input.resource,
    stableId,
    stableNo,
    name: input.name.trim(),
    lifecycleStatus: "ENABLED",
    lifecycleStatusLabel: "当前启用",
    lifecycleTone: "success",
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前",
    lockVersion: 1,
    currentRevision: {
      revisionId,
      revisionNo: 1,
      name: input.name.trim(),
      effectiveFrom,
      effectiveTo: input.effectiveTo,
      changeReason: input.changeReason.trim() || "新建",
      actor: ACTOR,
      fields: [
        { label: "名称", value: input.name.trim() },
        ...(input.fields
          ? Object.entries(input.fields).map(([label, value]) => ({
              label,
              value,
            }))
          : []),
      ],
    },
    revisionTimeline: [
      {
        id: revisionId,
        revisionNo: 1,
        revisionTiming: "CURRENT",
        timingLabel: "当前",
        nameSnapshot: input.name.trim(),
        actor: ACTOR,
        effectiveFrom,
        effectiveTo: input.effectiveTo,
        changeReason: input.changeReason.trim() || "新建",
        isCurrent: true,
        lifecycleAtRevision: "ENABLED",
      },
    ],
    selectorEligibility: listItem.selectorEligibility,
    usageSummary: {
      historicalReferenceCount: 0,
      note: "新建对象尚无历史引用。",
    },
    sensitiveFields: [],
    resourceFacts: [{ label: "创建人", value: ACTOR }],
    allowedActions: ["VIEW", "CREATE_REVISION", "DISABLE"],
    actionBlockers: [],
    auditEvents: [
      {
        id: `${stableId}_audit_1`,
        at: recordedAt,
        actor: ACTOR,
        action: "新建",
        detail: `v1 · ${input.changeReason.trim() || "新建"}`,
      },
    ],
    sections: ["overview", "versions", "relations", "audit"],
  }

  listOverlays.set(listKey(input.resource, stableId), listItem)
  centerOverlays.set(listKey(input.resource, stableId), center)
  const ids = createdIdsByResource.get(input.resource) ?? []
  ids.unshift(stableId)
  createdIdsByResource.set(input.resource, ids)

  const result: MasterDataMutationResult = {
    outcome: "succeeded",
    stableId,
    stableNo,
    revisionId,
    revisionNo: 1,
    revisionState: "CURRENT",
    effectiveFrom,
    recordedAt,
    actor: ACTOR,
    changeReason: input.changeReason.trim() || "新建",
    reference: `MD-CREATE-${stableNo}`,
    nextActions: ["查看详情", "形成新版本"],
  }
  idempotencyResults.set(input.idempotencyKey, result)
  return result
}

export function reviseW14Object(
  input: CreateRevisionInput
): MasterDataMutationResult {
  const cached = idempotencyResults.get(input.idempotencyKey)
  if (cached) return cached

  if (input.resource === "warehouses") {
    const blocked = rejectWarehouseWrite()
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  const center = getW14Center(input.resource, input.stableId)
  if (!center) {
    return {
      outcome: "unknown",
      message: "对象不存在或无权访问。",
      idempotencyKey: input.idempotencyKey,
    }
  }

  if (
    input.simulate === "conflict" ||
    input.expectedLockVersion !== center.lockVersion
  ) {
    const result: MasterDataMutationResult = {
      outcome: "conflict",
      message: "基础资料版本已变化，禁止静默覆盖。请刷新基准后重做。",
      serverLockVersion: center.lockVersion,
      serverRevisionNo: center.currentRevision.revisionNo,
    }
    idempotencyResults.set(input.idempotencyKey, result)
    return result
  }

  if (input.simulate === "overlap") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "EFFECTIVE_RANGE_OVERLAP",
      message: "生效区间与已有修订重叠。",
      detail: `冲突位置：当前 v${center.currentRevision.revisionNo}（${center.currentRevision.effectiveFrom} 起）。请调整生效区间。`,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "sku_signature" && input.resource === "products") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "SPEC_SIGNATURE_IMMUTABLE",
      message:
        "规格身份变化必须新建 SKU，不允许通过同一 SKU 修订改变 specification_signature。",
      detail: center.productConstraints
        ? `当前签名 ${center.productConstraints.specificationSignature}`
        : undefined,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "base_unit" && input.resource === "products") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "BASE_UNIT_LOCKED",
      message:
        "已被已生效单据使用的 SKU 不得修改基础单位。请「停用并新建 SKU」。",
      detail: center.productConstraints
        ? `当前基础单位 ${center.productConstraints.baseUnit}`
        : undefined,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  const newRevNo = center.currentRevision.revisionNo + 1
  const revisionId = `${input.stableId}_r${newRevNo}`
  const recordedAt = new Date().toISOString()
  const isFuture = input.effectiveFrom > new Date().toISOString().slice(0, 10)

  const nameSnapshot = input.name.trim()
  const changeReason = input.changeReason.trim()

  const nextCenter: MasterDataCenterView = {
    ...center,
    name: isFuture ? center.name : nameSnapshot,
    lockVersion: center.lockVersion + 1,
    revisionTiming: isFuture ? "FUTURE" : "CURRENT",
    revisionTimingLabel: isFuture ? "待生效" : "当前",
    currentRevision: isFuture
      ? center.currentRevision
      : {
          revisionId,
          revisionNo: newRevNo,
          name: nameSnapshot,
          effectiveFrom: input.effectiveFrom,
          effectiveTo: input.effectiveTo,
          changeReason,
          actor: ACTOR,
          fields: [
            { label: "名称", value: nameSnapshot },
            ...(input.fields
              ? Object.entries(input.fields).map(([label, value]) => ({
                  label,
                  value,
                }))
              : center.currentRevision.fields),
          ],
        },
    revisionTimeline: [
      {
        id: revisionId,
        revisionNo: newRevNo,
        revisionTiming: isFuture ? "FUTURE" : "CURRENT",
        timingLabel: isFuture ? "待生效" : "当前",
        nameSnapshot,
        actor: ACTOR,
        effectiveFrom: input.effectiveFrom,
        effectiveTo: input.effectiveTo,
        changeReason,
        isCurrent: !isFuture,
        lifecycleAtRevision: center.lifecycleStatus,
      },
      ...center.revisionTimeline.map((entry) => ({
        ...entry,
        isCurrent: isFuture ? entry.isCurrent : false,
        revisionTiming: isFuture
          ? entry.revisionTiming
          : entry.isCurrent
            ? ("HISTORICAL" as const)
            : entry.revisionTiming,
        timingLabel: isFuture
          ? entry.timingLabel
          : entry.isCurrent
            ? "已结束"
            : entry.timingLabel,
      })),
    ],
    auditEvents: [
      {
        id: `${revisionId}_audit`,
        at: recordedAt,
        actor: ACTOR,
        action: isFuture ? "形成待生效版本" : "形成新版本",
        detail: `v${newRevNo} · ${changeReason}`,
      },
      ...center.auditEvents,
    ],
  }

  const listRow =
    listW14Rows(input.resource).find((r) => r.stableId === input.stableId) ??
    null

  if (listRow) {
    const nextList: MasterDataListItem = {
      ...listRow,
      name: isFuture ? listRow.name : nameSnapshot,
      revisionNo: isFuture ? listRow.revisionNo : newRevNo,
      revisionTiming: isFuture ? "FUTURE" : "CURRENT",
      revisionTimingLabel: isFuture ? "待生效" : "当前",
      displayedRevisionId: revisionId,
      currentRevisionId: isFuture
        ? listRow.currentRevisionId
        : revisionId,
      effectiveFrom: isFuture ? listRow.effectiveFrom : input.effectiveFrom,
      effectiveTo: isFuture ? listRow.effectiveTo : input.effectiveTo,
      lockVersion: listRow.lockVersion + 1,
      metricTags: isFuture
        ? Array.from(new Set([...listRow.metricTags, "pending"]))
        : listRow.metricTags,
    }
    listOverlays.set(listKey(input.resource, input.stableId), nextList)
  }

  centerOverlays.set(listKey(input.resource, input.stableId), nextCenter)

  const result: MasterDataMutationResult = {
    outcome: "succeeded",
    stableId: input.stableId,
    stableNo: center.stableNo,
    revisionId,
    revisionNo: newRevNo,
    revisionState: isFuture ? "FUTURE" : "CURRENT",
    effectiveFrom: input.effectiveFrom,
    recordedAt,
    actor: ACTOR,
    changeReason,
    reference: `MD-REV-${center.stableNo}-v${newRevNo}`,
    nextActions: ["查看版本时间线", "核对选择器影响"],
  }
  idempotencyResults.set(input.idempotencyKey, result)
  return result
}

export function disableW14Object(
  input: DisableMasterDataInput
): MasterDataMutationResult {
  const cached = idempotencyResults.get(input.idempotencyKey)
  if (cached) return cached

  if (input.resource === "warehouses") {
    // Q1 fail-closed first; stock is secondary demo path for messaging.
    if (input.simulate === "warehouse_stock") {
      const center = getW14Center(input.resource, input.stableId)
      const stock = center?.warehouseStockSummary
      const blocked: MasterDataMutationResult = {
        outcome: "blocked",
        code: WAREHOUSE_WRITE_CODE,
        message: WAREHOUSE_WRITE_MESSAGE,
        detail: stock?.hasBlockingStock
          ? `同时存在库存占用：在库 ${stock.onHandQty} / 预占 ${stock.reservedQty}。`
          : undefined,
        drillHref: stock?.w10Href,
      }
      idempotencyResults.set(input.idempotencyKey, blocked)
      return blocked
    }
    const blocked = rejectWarehouseWrite()
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  const center = getW14Center(input.resource, input.stableId)
  if (!center) {
    return {
      outcome: "unknown",
      message: "对象不存在或无权访问。",
      idempotencyKey: input.idempotencyKey,
    }
  }

  if (center.lifecycleStatus === "DISABLED") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "ALREADY_DISABLED",
      message: "对象已停用；停用不是删除，历史版本仍可只读打开。",
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (
    input.simulate === "conflict" ||
    input.expectedLockVersion !== center.lockVersion
  ) {
    const result: MasterDataMutationResult = {
      outcome: "conflict",
      message: "基础资料版本已变化，禁止静默覆盖。",
      serverLockVersion: center.lockVersion,
      serverRevisionNo: center.currentRevision.revisionNo,
    }
    idempotencyResults.set(input.idempotencyKey, result)
    return result
  }

  const newRevNo = center.currentRevision.revisionNo + 1
  const revisionId = `${input.stableId}_r${newRevNo}`
  const recordedAt = new Date().toISOString()
  const changeReason = input.changeReason.trim()

  // Preserve historical name snapshots on timeline; current name stays for disabled object.
  const nextCenter: MasterDataCenterView = {
    ...center,
    lifecycleStatus: "DISABLED",
    lifecycleStatusLabel: "当前停用",
    lifecycleTone: "neutral",
    lockVersion: center.lockVersion + 1,
    currentRevision: {
      revisionId,
      revisionNo: newRevNo,
      name: center.name,
      effectiveFrom: input.effectiveFrom,
      changeReason,
      actor: ACTOR,
      fields: center.currentRevision.fields,
    },
    revisionTimeline: [
      {
        id: revisionId,
        revisionNo: newRevNo,
        revisionTiming: "CURRENT",
        timingLabel: "当前",
        nameSnapshot: center.name,
        actor: ACTOR,
        effectiveFrom: input.effectiveFrom,
        changeReason,
        isCurrent: true,
        lifecycleAtRevision: "DISABLED",
      },
      ...center.revisionTimeline.map((entry) => ({
        ...entry,
        isCurrent: false,
        revisionTiming:
          entry.isCurrent && entry.revisionTiming === "CURRENT"
            ? ("HISTORICAL" as const)
            : entry.revisionTiming,
        timingLabel:
          entry.isCurrent && entry.revisionTiming === "CURRENT"
            ? "已结束"
            : entry.timingLabel,
      })),
    ],
    selectorEligibility: center.selectorEligibility.map((s) => ({
      ...s,
      eligible: false,
      blockerCodes: [...s.blockerCodes, "LIFECYCLE_DISABLED"],
      reason: "当前停用",
    })),
    allowedActions: ["VIEW", "CREATE_REVISION"],
    actionBlockers: [
      {
        action: "DISABLE",
        code: "ALREADY_DISABLED",
        message: "对象已停用；身份与历史版本永久保留。",
      },
    ],
    auditEvents: [
      {
        id: `${revisionId}_audit`,
        at: recordedAt,
        actor: ACTOR,
        action: "停用",
        detail: `v${newRevNo} · ${changeReason}`,
      },
      ...center.auditEvents,
    ],
    usageSummary: {
      ...center.usageSummary,
      note: "停用非删除：已引用身份保留并可只读打开历史版本。",
    },
  }

  const listRow = listW14Rows(input.resource).find(
    (r) => r.stableId === input.stableId
  )
  if (listRow) {
    listOverlays.set(listKey(input.resource, input.stableId), {
      ...listRow,
      lifecycleStatus: "DISABLED",
      lifecycleStatusLabel: "当前停用",
      lifecycleTone: "neutral",
      revisionNo: newRevNo,
      currentRevisionId: revisionId,
      displayedRevisionId: revisionId,
      lockVersion: listRow.lockVersion + 1,
      primaryBlocker: "已停用：不可进入业务选择器",
      selectorEligibility: listRow.selectorEligibility.map((s) => ({
        ...s,
        eligible: false,
        blockerCodes: [...s.blockerCodes, "LIFECYCLE_DISABLED"],
        reason: "当前停用",
      })),
      allowedActions: ["VIEW", "CREATE_REVISION", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "DISABLE",
          code: "ALREADY_DISABLED",
          message: "对象已停用。",
        },
      ],
      metricTags: ["disabled"],
    })
  }

  centerOverlays.set(listKey(input.resource, input.stableId), nextCenter)

  const result: MasterDataMutationResult = {
    outcome: "succeeded",
    stableId: input.stableId,
    stableNo: center.stableNo,
    revisionId,
    revisionNo: newRevNo,
    revisionState: "CURRENT",
    effectiveFrom: input.effectiveFrom,
    recordedAt,
    actor: ACTOR,
    changeReason,
    reference: `MD-DIS-${center.stableNo}-v${newRevNo}`,
    nextActions: ["只读打开历史版本", "返回列表"],
  }
  idempotencyResults.set(input.idempotencyKey, result)
  return result
}

export function queryW14Idempotency(
  key: string
): MasterDataMutationResult | null {
  return idempotencyResults.get(key) ?? null
}

export function queryW14Selector(
  scene: SelectorQueryScene
): SelectorQueryResult {
  const asOf = new Date().toISOString()
  const pick = (
    resource: MasterDataResource,
    map: (row: MasterDataListItem) => SelectorCandidate | null
  ): SelectorCandidate[] =>
    listW14Rows(resource)
      .map(map)
      .filter((x): x is SelectorCandidate => Boolean(x))

  if (scene === "sales_pick") {
    return {
      scene,
      asOf,
      note: "仅返回启用、当前有效、区域/履约匹配的精确可销售项目版本；提交时再校验。",
      candidates: pick("sellable-items", (row) => {
        if (row.lifecycleStatus !== "ENABLED") return null
        if (row.revisionTiming === "FUTURE" && row.scheduledLifecycleStatus === "DISABLED") {
          // current version still eligible
        }
        const elig = row.selectorEligibility.find((s) => s.context === "sales_pick")
        if (elig && !elig.eligible) return null
        return {
          stableId: row.stableId,
          stableNo: row.stableNo,
          name: row.name,
          revisionId: row.currentRevisionId,
          revisionNo: row.revisionNo,
          eligible: true,
        }
      }),
    }
  }

  if (scene === "procurement_supplier") {
    return {
      scene,
      asOf,
      note: "校验供应商角色启用、能力、适用资质与业务日期；列表可用≠提交通过。",
      candidates: pick("suppliers", (row) => {
        const elig = row.selectorEligibility.find(
          (s) => s.context === "procurement_supplier"
        )
        if (!elig?.eligible) {
          return {
            stableId: row.stableId,
            stableNo: row.stableNo,
            name: row.name,
            revisionId: row.currentRevisionId,
            revisionNo: row.revisionNo,
            eligible: false,
            reason: elig?.reason ?? "不可用",
          }
        }
        return {
          stableId: row.stableId,
          stableNo: row.stableNo,
          name: row.name,
          revisionId: row.currentRevisionId,
          revisionNo: row.revisionNo,
          eligible: true,
          reason: elig.reason,
        }
      }),
    }
  }

  if (scene === "sku_pick") {
    return {
      scene,
      asOf,
      note: "SKU 启用、规格身份稳定、基础单位存在。",
      candidates: pick("products", (row) => {
        const ok = row.lifecycleStatus === "ENABLED"
        return {
          stableId: row.stableId,
          stableNo: row.stableNo,
          name: row.name,
          revisionId: row.currentRevisionId,
          revisionNo: row.revisionNo,
          eligible: ok,
          reason: ok ? undefined : "当前停用",
        }
      }),
    }
  }

  if (scene === "voucher_category") {
    return {
      scene,
      asOf,
      note: "product_kind=VOUCHER；不读取商城玩法。",
      candidates: pick("voucher-categories", (row) => ({
        stableId: row.stableId,
        stableNo: row.stableNo,
        name: row.name,
        revisionId: row.currentRevisionId,
        revisionNo: row.revisionNo,
        eligible: row.lifecycleStatus === "ENABLED",
        reason:
          row.lifecycleStatus === "ENABLED"
            ? "ERP 销售项 · 无玩法字段"
            : "当前停用",
      })),
    }
  }

  return {
    scene: "warehouse_pick",
    asOf,
    note: "仓库当前有效且数据范围允许；库存操作另由库存台账校验。",
    candidates: pick("warehouses", (row) => ({
      stableId: row.stableId,
      stableNo: row.stableNo,
      name: row.name,
      revisionId: row.currentRevisionId,
      revisionNo: row.revisionNo,
      eligible: row.lifecycleStatus === "ENABLED",
      reason:
        row.lifecycleStatus === "ENABLED"
          ? "可查询；写操作暂不可用"
          : "当前停用",
    })),
  }
}
