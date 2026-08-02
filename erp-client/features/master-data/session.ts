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
    detail: "仓库资料暂不可维护，任何角色都不能改。",
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
      message: "生效期间与已有内容重叠，无法保存。",
      detail: `与当前 v1（从 ${input.effectiveFrom} 起）冲突，请调整生效开始或结束日期。`,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "sku_signature" && input.resource === "products") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "SPEC_SIGNATURE_IMMUTABLE",
      message: "规格变更需要新建商品，不能在新建时伪造规格变更。",
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
    revisionTimingLabel: "当前生效",
    currentRevisionId: revisionId,
    displayedRevisionId: revisionId,
    revisionNo: 1,
    effectiveFrom,
    effectiveTo: input.effectiveTo,
    keyFacts: [
      { label: "分类", value: resourceLabel(input.resource) },
      { label: "说明", value: "本次新建" },
    ],
    selectorEligibility: [
      {
        context: "default",
        contextLabel: "业务选用",
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
    revisionTimingLabel: "当前生效",
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
        timingLabel: "当前生效",
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
      note: "新建资料尚无业务引用。",
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
    nextActions: ["查看详情", "更新资料"],
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
      message: "资料不存在或无权访问。",
      idempotencyKey: input.idempotencyKey,
    }
  }

  if (
    input.simulate === "conflict" ||
    input.expectedLockVersion !== center.lockVersion
  ) {
    const result: MasterDataMutationResult = {
      outcome: "conflict",
      message: "资料已被他人更新，请刷新后重新填写。",
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
      message: "生效期间与已有内容重叠。",
      detail: `与当前 v${center.currentRevision.revisionNo}（${center.currentRevision.effectiveFrom} 起）冲突，请调整生效日期。`,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "sku_signature" && input.resource === "products") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "SPEC_SIGNATURE_IMMUTABLE",
      message: "规格变更需要新建商品，不能在同一商品上改规格。",
      detail: center.productConstraints
        ? `当前规格标识 ${center.productConstraints.specificationSignature}`
        : undefined,
    }
    idempotencyResults.set(input.idempotencyKey, blocked)
    return blocked
  }

  if (input.simulate === "base_unit" && input.resource === "products") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "BASE_UNIT_LOCKED",
      message: "已被业务单据使用的商品不能改基础单位。请先停用，再新建商品。",
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
    revisionTimingLabel: isFuture ? "待生效" : "当前生效",
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
        timingLabel: isFuture ? "待生效" : "当前生效",
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
        action: isFuture ? "预约更新" : "更新资料",
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
      revisionTimingLabel: isFuture ? "待生效" : "当前生效",
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
    nextActions: ["查看变更历史", "返回列表"],
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
      message: "资料不存在或无权访问。",
      idempotencyKey: input.idempotencyKey,
    }
  }

  if (center.lifecycleStatus === "DISABLED") {
    const blocked: MasterDataMutationResult = {
      outcome: "blocked",
      code: "ALREADY_DISABLED",
      message: "资料已停用；不是删除，历史记录仍可查看。",
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
      message: "资料已被他人更新，请刷新后重试。",
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
        timingLabel: "当前生效",
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
        message: "资料已停用；编号与历史记录永久保留。",
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
      note: "停用不是删除：历史业务引用仍可查看。",
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
      primaryBlocker: "已停用：业务页面选不到",
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
          message: "资料已停用。",
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
    nextActions: ["查看变更历史", "返回列表"],
  }
  idempotencyResults.set(input.idempotencyKey, result)
  return result
}

export function queryW14Idempotency(
  key: string
): MasterDataMutationResult | null {
  return idempotencyResults.get(key) ?? null
}
