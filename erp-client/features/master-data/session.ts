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
  BrandFields,
  CategoryFields,
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  LifecycleStatus,
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataListResult,
  MasterDataMutationResult,
  MasterDataResource,
  ProductDetailView,
  ProductFields,
  ProductKind,
  ProductSkuFields,
  SkuRevisionRecord,
} from "@/features/master-data/types"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import {
  computeSpecificationSignature,
  productListFacts,
} from "@/features/master-data/product-model"
import {
  RESOURCE_FIELDS,
  resourceFieldsToFacts,
  resourceFieldsToListFacts,
} from "@/features/master-data/resource-fields"

function resolveCategoryParentName(parentId?: string): string {
  if (!parentId) return "（根分类）"
  const parent = listW14Rows("categories").find((row) => row.stableId === parentId)
  return parent?.name ?? "（未知上级）"
}

/** 分类/品牌字典事实：用业务文案而非原始 ID。 */
function dictionaryFacts(
  resource: MasterDataResource,
  fields: CreateMasterDataInput["fields"]
): ReadonlyArray<{ label: string; value: string }> {
  if (resource === "categories") {
    const f = fields as CategoryFields
    return [
      { label: "分类代码", value: f.code },
      { label: "上级分类", value: resolveCategoryParentName(f.parentId) },
      ...(f.productKind
        ? [{ label: "适用商品类型", value: f.productKind }]
        : []),
    ]
  }
  if (resource === "brands") {
    const f = fields as BrandFields
    return [
      { label: "品牌代码", value: f.code },
      ...(f.logo ? [{ label: "品牌 Logo", value: f.logo }] : []),
    ]
  }
  return resourceFieldsToFacts(resource, fields)
}
const listOverlays = new Map<string, MasterDataListItem>()
const centerOverlays = new Map<string, MasterDataCenterView>()
const createdIdsByResource = new Map<MasterDataResource, string[]>()
const idempotencyResults = new Map<string, MasterDataMutationResult>()

const ACTOR = "当前用户"

function cloneProductDetail(detail: ProductDetailView): ProductDetailView {
  return {
    ...detail,
    carouselImages: [...detail.carouselImages],
    detailImages: [...detail.detailImages],
    specs: detail.specs.map((spec) => ({
      name: spec.name,
      values: [...spec.values],
    })),
    skus: detail.skus.map((sku) => ({
      ...sku,
      attributeValues: [...sku.attributeValues],
    })),
  }
}

function productSnapshot(fields: ProductFields): ProductDetailView {
  return cloneProductDetail({
    description: fields.description,
    baseUnitId: fields.baseUnitId,
    baseUnitCode: fields.baseUnitCode,
    baseUnit: fields.baseUnit,
    categoryId: fields.categoryId,
    category: fields.category,
    brandId: fields.brandId,
    brand: fields.brand,
    carouselImages: fields.carouselImages,
    detailImages: fields.detailImages,
    specs: fields.specs,
    skus: fields.skus,
  })
}

/** 历史上已移除（停用）的规格签名 → 原 skuId；签名再次出现时复用原身份。 */
const retiredSkuIdsBySignature = new Map<string, string>()

/** mock 落库的 `sku_revision`（公司 SKU 修订），按 skuId 分组、修订时间有序。 */
const skuRevisionStore = new Map<string, MutableSkuRevisionRecord[]>()

type MutableSkuRevisionRecord = {
  skuId: string
  skuNo: string
  specificationSignature: string
  salesVisiblePriceGross?: string
  marketPrice?: string
  lifecycleStatus: LifecycleStatus
  revisionId: string
  effectiveFrom: string
  recordedAt: string
  isCurrent: boolean
}

/**
 * W14 拥有稳定的公司 SKU 身份。供应商目录只能引用该 ID，绝不允许发明第二套
 * 公司 SKU 身份；`sku_no` 只是全局唯一业务编码，不能作为身份恢复或重绑键。
 *
 * 身份匹配只按规范化 `specification_signature`：签名未变的组合延续原 skuId；
 * 新签名新建 SKU；历史已停用签名再次出现时复用原 skuId（mock 层直接复用，
 * 显式重新启用确认流程见遗留说明）。移除的组合在 `persistSkuRevisions`
 * 中保留历史并停用旧 SKU。
 */
function withStableSkuIds(
  fields: ProductFields,
  stableProductId: string,
  previous?: ProductDetailView,
): ProductFields {
  const previousBySignature = new Map<string, string>()
  for (const prevSku of previous?.skus ?? []) {
    if (!prevSku.skuId) continue
    const signature =
      prevSku.specificationSignature ??
      computeSpecificationSignature(previous!.specs, prevSku.attributeValues)
    if (!previousBySignature.has(signature)) {
      previousBySignature.set(signature, prevSku.skuId)
    }
  }
  const usedIds = new Set<string>([
    ...fields.skus.flatMap((sku) => (sku.skuId ? [sku.skuId] : [])),
    ...(previous?.skus ?? []).flatMap((sku) => (sku.skuId ? [sku.skuId] : [])),
    ...retiredSkuIdsBySignature.values(),
  ])
  const nextSkus = fields.skus.map((sku) => {
    const signature =
      sku.specificationSignature ??
      computeSpecificationSignature(fields.specs, sku.attributeValues)
    if (sku.skuId) {
      usedIds.add(sku.skuId)
      return { ...sku, specificationSignature: signature }
    }

    const matchedId =
      previousBySignature.get(signature) ??
      retiredSkuIdsBySignature.get(signature)
    if (matchedId) {
      usedIds.add(matchedId)
      return { ...sku, skuId: matchedId, specificationSignature: signature }
    }

    // 新建 SKU：跳过全部历史/现有/停用身份，顺序取下一个空闲序号。
    let suffix = 1
    let skuId = `${stableProductId}_sku_${String(suffix).padStart(2, "0")}`
    while (usedIds.has(skuId)) {
      suffix += 1
      skuId = `${stableProductId}_sku_${String(suffix).padStart(2, "0")}`
    }
    usedIds.add(skuId)
    return { ...sku, skuId, specificationSignature: signature }
  })
  recordRetiredSignatures(previous, nextSkus)
  return { ...fields, skus: nextSkus }
}

/** 提交后不再出现的旧签名：保留历史（revisionTimeline 快照）并记录停用身份。 */
function recordRetiredSignatures(
  previous: ProductDetailView | undefined,
  nextSkus: readonly ProductSkuFields[],
): void {
  if (!previous) return
  const nextSignatures = new Set(
    nextSkus.map((sku) => sku.specificationSignature ?? ""),
  )
  for (const prevSku of previous.skus) {
    if (!prevSku.skuId) continue
    const signature =
      prevSku.specificationSignature ??
      computeSpecificationSignature(previous.specs, prevSku.attributeValues)
    if (nextSignatures.has(signature)) continue
    retiredSkuIdsBySignature.set(signature, prevSku.skuId)
  }
}

/**
 * 复合提交的 mock 落库：把每个 SKU 的 `salePrice`（表单草稿字段）写入
 * `sku_revision.sales_visible_price_gross`，并落 marketPrice、签名与生命周期；
 * 移除的规格组合追加停用修订（保留历史，不物理删除）。
 */
function persistSkuRevisions(input: {
  fields: ProductFields
  revisionId: string
  effectiveFrom: string
  previous?: ProductDetailView
}): void {
  const { fields, revisionId, effectiveFrom, previous } = input
  const recordedAt = new Date().toISOString()
  const submittedSignatures = new Set(
    fields.skus.map(
      (sku) =>
        sku.specificationSignature ??
        computeSpecificationSignature(fields.specs, sku.attributeValues),
    ),
  )

  for (const prevSku of previous?.skus ?? []) {
    if (!prevSku.skuId) continue
    const signature =
      prevSku.specificationSignature ??
      computeSpecificationSignature(previous!.specs, prevSku.attributeValues)
    if (submittedSignatures.has(signature)) continue
    const entries = skuRevisionStore.get(prevSku.skuId) ?? []
    for (const entry of entries) entry.isCurrent = false
    entries.push({
      skuId: prevSku.skuId,
      skuNo: prevSku.skuNo,
      specificationSignature: signature,
      salesVisiblePriceGross: prevSku.salePrice,
      marketPrice: prevSku.marketPrice,
      lifecycleStatus: "DISABLED",
      revisionId,
      effectiveFrom,
      recordedAt,
      isCurrent: true,
    })
    skuRevisionStore.set(prevSku.skuId, entries)
  }

  for (const sku of fields.skus) {
    if (!sku.skuId) continue
    const signature =
      sku.specificationSignature ??
      computeSpecificationSignature(fields.specs, sku.attributeValues)
    const entries = skuRevisionStore.get(sku.skuId) ?? []
    for (const entry of entries) entry.isCurrent = false
    entries.push({
      skuId: sku.skuId,
      skuNo: sku.skuNo,
      specificationSignature: signature,
      salesVisiblePriceGross: sku.salePrice,
      marketPrice: sku.marketPrice,
      lifecycleStatus: sku.lifecycleStatus,
      revisionId,
      effectiveFrom,
      recordedAt,
      isCurrent: true,
    })
    skuRevisionStore.set(sku.skuId, entries)
  }
}

/** 商品提交的商品类型与分类兼容性校验（fail-closed）。 */
function productKindIssue(
  fields: ProductFields,
): { code: string; message: string; detail?: string } | null {
  if (!fields.productKind) {
    return {
      code: "PRODUCT_KIND_REQUIRED",
      message: "请选择商品类型后再保存。",
      detail: "商品类型决定商品业务作用，保存后不可修改。",
    }
  }
  const label = PRODUCT_KIND_LABELS[fields.productKind]
  const categoryRow = listW14Rows("categories").find(
    (row) => row.stableId === fields.categoryId,
  )
  const allowedKind = categoryRow?.productKind
  if (allowedKind && label && allowedKind !== label) {
    return {
      code: "CATEGORY_KIND_INCOMPATIBLE",
      message: `分类「${categoryRow.name}」不兼容商品类型「${label}」，请更换分类或商品类型。`,
    }
  }
  return null
}

function blockedResult(
  input: { idempotencyKey: string },
  issue: { code: string; message: string; detail?: string },
): MasterDataMutationResult {
  const blocked: MasterDataMutationResult = {
    outcome: "blocked",
    code: issue.code,
    message: issue.message,
    detail: issue.detail,
  }
  idempotencyResults.set(input.idempotencyKey, blocked)
  return blocked
}

function listKey(resource: MasterDataResource, stableId: string) {
  return `${resource}:${stableId}`
}

/** 解析 YYYY-MM-DD 为当天 00:00 的毫秒时间戳；格式异常返回 0。 */
function parseDateOnly(value: string): number {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (!match) return 0
  return new Date(
    Number(match[1]),
    Number(match[2]) - 1,
    Number(match[3])
  ).getTime()
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
  if (!seed.productDetail) return seed
  return {
    ...seed,
    productDetail: cloneProductDetail(seed.productDetail),
    revisionTimeline: seed.revisionTimeline.map((entry) => ({
      ...entry,
      // Older fixture revisions still receive their own immutable object copy.
      // Session revisions below preserve the exact submitted content per version.
      productSnapshot: cloneProductDetail(entry.productSnapshot ?? seed.productDetail!),
    })),
  }
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
        categories: true,
        brands: true,
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
    products: "SPU-NEW",
    categories: "CAT-NEW",
    brands: "BRD-NEW",
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

  const seq = (createdIdsByResource.get(input.resource)?.length ?? 0) + 1
  const stableId = `${input.resource.replace(/-/g, "_")}_new_${seq}`
  const stableNo = nextStableNo(input.resource, seq)
  const revisionId = `${stableId}_r1`
  const recordedAt = new Date().toISOString()
  const effectiveFrom = input.effectiveFrom

  const submittedProductFields =
    input.resource === "products"
      ? (input.fields as ProductFields)
      : undefined
  if (submittedProductFields) {
    const kindIssue = productKindIssue(submittedProductFields)
    if (kindIssue) return blockedResult(input, kindIssue)
  }
  const productFields = submittedProductFields
    ? withStableSkuIds(submittedProductFields, stableId)
    : undefined
  const categoryFields =
    input.resource === "categories"
      ? (input.fields as CategoryFields)
      : undefined
  const brandFields =
    input.resource === "brands" ? (input.fields as BrandFields) : undefined
  const listFacts = productFields
    ? productListFacts(productFields)
    : input.resource === "categories" || input.resource === "brands"
      ? dictionaryFacts(input.resource, input.fields)
      : resourceFieldsToListFacts(input.resource, input.fields)
  const revisionFacts = productFields
    ? productListFacts(productFields)
    : input.resource === "categories" || input.resource === "brands"
      ? dictionaryFacts(input.resource, input.fields)
      : resourceFieldsToFacts(input.resource, input.fields)
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
    keyFacts:
      listFacts.length > 0
        ? [...listFacts]
        : [
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
    dictionaryCode: categoryFields?.code ?? brandFields?.code,
    parentStableId: categoryFields?.parentId,
    productKind: categoryFields?.productKind,
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
        ...revisionFacts,
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
        productSnapshot: productFields ? productSnapshot(productFields) : undefined,
      },
    ],
    selectorEligibility: listItem.selectorEligibility,
    usageSummary: {
      historicalReferenceCount: 0,
      note: "新建资料尚无业务引用。",
    },
    sensitiveFields: [],
    resourceFacts: [...revisionFacts, { label: "创建人", value: ACTOR }],
    productConstraints: productFields
      ? {
          baseUnit: productFields.baseUnit,
          hasFormalReferences: false,
          skuCount: productFields.skus.length,
        }
      : undefined,
    productDetail: productFields ? productSnapshot(productFields) : undefined,
    productKind: productFields ? productFields.productKind || undefined : undefined,
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

  if (productFields) {
    persistSkuRevisions({
      fields: productFields,
      revisionId,
      effectiveFrom,
    })
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
  // 用日期对象比较，避免「2026/08/05」这类非标准格式被字符串大小误判为未来。
  const isFuture =
    parseDateOnly(input.effectiveFrom) > new Date().getTime()

  const nameSnapshot = input.name.trim()
  const changeReason = input.changeReason.trim()
  const submittedProductFields =
    input.resource === "products"
      ? (input.fields as ProductFields)
      : undefined
  if (submittedProductFields) {
    const storedKind: ProductKind | undefined = center.productKind
    if (storedKind && submittedProductFields.productKind !== storedKind) {
      const blocked: MasterDataMutationResult = {
        outcome: "blocked",
        code: "PRODUCT_KIND_IMMUTABLE",
        message: "商品类型创建后不可修改。",
        detail: `当前商品类型为「${PRODUCT_KIND_LABELS[storedKind]}」。`,
      }
      idempotencyResults.set(input.idempotencyKey, blocked)
      return blocked
    }
    const kindIssue = productKindIssue(submittedProductFields)
    if (kindIssue) return blockedResult(input, kindIssue)
  }
  const productFields = submittedProductFields
    ? withStableSkuIds(submittedProductFields, input.stableId, center.productDetail)
    : undefined
  const categoryFields =
    input.resource === "categories"
      ? (input.fields as CategoryFields)
      : undefined
  const brandFields =
    input.resource === "brands" ? (input.fields as BrandFields) : undefined
  const revisionFacts = productFields
    ? productListFacts(productFields)
    : input.resource === "categories" || input.resource === "brands"
      ? dictionaryFacts(input.resource, input.fields)
      : resourceFieldsToFacts(input.resource, input.fields)
  /**
   * 提交字段（含清空）覆盖旧值；仅保留不在表单字段集合内的历史事实
   * （如 seed 中的“product_kind”“边界”等表单未覆盖标签），避免误清。
   * 表单字段集合 = 各 def 的 label + aliases + 基础“名称”。
   */
  function mergeRevisionFacts(
    next: ReadonlyArray<{ label: string; value: string }>,
    previous: ReadonlyArray<{ label: string; value: string }>,
    resource: MasterDataResource
  ): ReadonlyArray<{ label: string; value: string }> {
    const nextLabels = new Set(next.map((fact) => fact.label))
    const defLabels = new Set(["名称"])
    for (const def of RESOURCE_FIELDS[resource]) {
      defLabels.add(def.label)
      for (const alias of def.aliases ?? []) defLabels.add(alias)
    }
    return [
      ...next,
      ...previous.filter(
        (fact) => !nextLabels.has(fact.label) && !defLabels.has(fact.label)
      ),
    ]
  }

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
            ...mergeRevisionFacts(
              revisionFacts,
              center.currentRevision.fields,
              input.resource
            ),
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
        productSnapshot: productFields ? productSnapshot(productFields) : undefined,
      },
      ...center.revisionTimeline.map((entry) => ({
        ...entry,
        productSnapshot:
          entry.productSnapshot ??
          (entry.isCurrent && center.productDetail
            ? cloneProductDetail(center.productDetail)
            : undefined),
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
    productConstraints:
      !isFuture && productFields
        ? {
            baseUnit: productFields.baseUnit,
            hasFormalReferences:
              center.productConstraints?.hasFormalReferences ?? false,
            skuCount: productFields.skus.length,
          }
        : center.productConstraints,
    productDetail:
      !isFuture && productFields
        ? productSnapshot(productFields)
        : center.productDetail,
  }

  const listRow =
    listW14Rows(input.resource).find((r) => r.stableId === input.stableId) ??
    null

  if (listRow) {
    const nextListFacts = productFields
      ? productListFacts(productFields)
      : input.resource === "categories" || input.resource === "brands"
        ? dictionaryFacts(input.resource, input.fields)
        : resourceFieldsToListFacts(input.resource, input.fields)
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
      keyFacts: isFuture
        ? listRow.keyFacts
        : mergeRevisionFacts(nextListFacts, listRow.keyFacts, input.resource),
      metricTags: isFuture
        ? Array.from(new Set([...listRow.metricTags, "pending"]))
        : listRow.metricTags,
      dictionaryCode: isFuture
        ? listRow.dictionaryCode
        : (categoryFields?.code ?? brandFields?.code ?? listRow.dictionaryCode),
      parentStableId: isFuture
        ? listRow.parentStableId
        : categoryFields
          ? categoryFields.parentId
          : listRow.parentStableId,
      productKind: isFuture
        ? listRow.productKind
        : (categoryFields?.productKind ?? listRow.productKind),
    }
    listOverlays.set(listKey(input.resource, input.stableId), nextList)
  }

  centerOverlays.set(listKey(input.resource, input.stableId), nextCenter)

  if (productFields) {
    persistSkuRevisions({
      fields: productFields,
      revisionId,
      effectiveFrom: input.effectiveFrom,
      previous: center.productDetail,
    })
  }

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

/** 读取 mock 落库的 `sku_revision` 记录（按 skuId）。 */
export function getW14SkuRevisions(
  skuId: string
): readonly SkuRevisionRecord[] | undefined {
  const entries = skuRevisionStore.get(skuId)
  return entries ? entries.map((entry) => ({ ...entry })) : undefined
}
