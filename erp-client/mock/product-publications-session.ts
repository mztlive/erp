/**
 * W22 product publication · session-only formal action state.
 * Not persisted; survives SPA navigation within the tab only.
 */

import type {
  ManualPauseCommand,
  ManualPauseResult,
  PublishRevisionCommand,
  PublishRevisionResult,
  ResolvePublishUnknownCommand,
  RetryDeliveryCommand,
  RetryDeliveryResult,
  SystemSafetyPauseOperationView,
  SystemSafetyPauseTrigger,
} from "@/features/product-publications/types"
import {
  DELIVERY_STATUS_LABEL,
  DELIVERY_STATUS_TONE,
  PUBLICATION_STATUS_LABEL,
  PUBLICATION_STATUS_TONE,
  SALE_STATUS_LABEL,
} from "@/features/product-publications/types"
import {
  PUBLICATION_SEEDS,
  type PublicationSeed,
} from "@/mock/product-publications"
import { SUPPLIER_CATALOG_SEED } from "@/mock/supplier-catalog"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"

const w22Overrides = new Map<string, PublicationSeed>()
const w22SessionCreated: PublicationSeed[] = []
const w22PublishIdempotency = new Map<string, PublishRevisionResult>()
const w22PendingPublish = new Map<string, PublishRevisionCommand>()
const w22SafetyPauseIdempotency = new Map<
  string,
  SystemSafetyPauseOperationView
>()
let w22RevSeq = 100
let w22DlvSeq = 100
let w22OpSeq = 100

function cloneSeed(seed: PublicationSeed): PublicationSeed {
  return structuredClone(seed)
}

function baseOrOverride(publicationId: string): PublicationSeed | null {
  const over = w22Overrides.get(publicationId)
  if (over) return over
  const session = w22SessionCreated.find(
    (s) => s.row.publicationId === publicationId
  )
  if (session) return session
  const seed = PUBLICATION_SEEDS.find(
    (s) => s.row.publicationId === publicationId
  )
  return seed ? cloneSeed(seed) : null
}

function resolveFixedOffering(
  seed: PublicationSeed,
  offeringRevisionId: string
): PublicationSeed["row"]["fixedOffering"] | null {
  for (const revision of seed.revisions) {
    if (revision.fixedOffering.offeringRevisionId === offeringRevisionId) {
      return structuredClone(revision.fixedOffering)
    }
  }
  for (const item of SUPPLIER_CATALOG_SEED) {
    const offering = item.offering
    if (!offering) continue
    const revision = [
      ...(offering.currentRevision ? [offering.currentRevision] : []),
      ...offering.revisionHistory,
    ].find((candidate) => candidate.offeringRevisionId === offeringRevisionId)
    if (!revision) continue
    return {
      offeringRevisionId,
      supplierName: item.supplierProduct.supplier.name,
      availability: revision.availabilityStatus.toLowerCase(),
      availabilityLabel:
        revision.availabilityStatus === "AVAILABLE"
          ? "可供"
          : revision.availabilityStatus === "STALE"
            ? "数据过期"
            : "不可供",
      supplyPriceVisible: revision.supplyPriceGross != null,
      supplyPriceGross: revision.supplyPriceGross ?? undefined,
      supplierMoq: revision.minimumOrderQuantity,
    }
  }
  return null
}

function validatePublishContent(
  seed: PublicationSeed,
  command: PublishRevisionCommand,
  fixedOffering: PublicationSeed["row"]["fixedOffering"] | null
): string | null {
  const content = command.content
  if (!seed.allowedActions.includes("PUBLISH")) return "当前对象无发布权限。"
  if (
    !content.skuRevisionId.trim() ||
    !content.categoryId.trim() ||
    !content.name.trim() ||
    !content.specification.trim() ||
    !content.salesDescription.trim() ||
    !content.baseUnitCode.trim()
  ) {
    return "商品修订、类目、名称、规格、销售说明和基础单位均为必填。"
  }
  if (!fixedOffering) return "固定供给修订不存在或当前无权使用。"
  if (
    content.saleStatus === "ON_SALE" &&
    fixedOffering.availability !== "available"
  ) {
    return "固定供给当前不可供或数据过期，不能提交上架。"
  }
  if (content.salesRegion.length === 0) return "至少选择一个可销售区域。"
  if (!content.validFrom.trim()) return "发布生效时间为必填。"
  if (content.validTo && content.validTo <= content.validFrom) {
    return "发布失效时间必须晚于生效时间。"
  }
  try {
    parseDecimal(content.minimumPurchaseQuantity, { maxScale: 6 })
    parseDecimal(content.salesPriceGross, { maxScale: 4 })
    parseDecimal(content.salesTaxRate, { maxScale: 6 })
    if (compareDecimal(content.minimumPurchaseQuantity, "0", 6) <= 0) {
      return "最小购买量必须大于 0。"
    }
    if (compareDecimal(content.salesPriceGross, "0", 4) <= 0) {
      return "含税销售价必须大于 0。"
    }
    if (
      compareDecimal(content.salesTaxRate, "0", 6) < 0 ||
      compareDecimal(content.salesTaxRate, "1", 6) > 0
    ) {
      return "销项税率必须为 0 到 1 的十进制数。"
    }
  } catch {
    return "价格、税率或最小购买量不是合法定点小数。"
  }

  const mainMedia = content.media.filter((media) => media.mediaRole === "MAIN")
  if (mainMedia.length !== 1 || content.media.some((media) => !media.altText.trim())) {
    return "必须提供恰好一张主图，且每张图片都要有图片说明。"
  }
  const knownMedia = seed.revisions.flatMap((revision) => revision.media)
  if (
    content.media.some((media) => {
      const asset = knownMedia.find((candidate) => candidate.fileAssetId === media.fileAssetId)
      return !asset || asset.securityScanStatus !== "PASSED"
    })
  ) {
    return "存在未通过安全扫描或不在当前授权范围内的媒体。"
  }
  return null
}

export function listW22SessionPublications(): PublicationSeed[] {
  return [...w22SessionCreated]
}

export function getW22PublicationOverride(
  publicationId: string
): PublicationSeed | null {
  return w22Overrides.get(publicationId) ?? null
}

function recomputePublishGate(
  seed: PublicationSeed,
  content: PublishRevisionCommand["content"]
): PublicationSeed["publishGate"] {
  if (
    seed.row.publicationStatus === "SAFETY_PAUSED" ||
    seed.row.safetyPause != null
  ) {
    if (content.saleStatus === "ON_SALE") {
      return {
        kind: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
        gateVersion: seed.publishGate.gateVersion,
        submissionKind: "RECOVERY",
        blocker: {
          code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
          message:
            "安全暂停后的恢复发起人与确认人尚未确认，不能提交上架。",
        },
      }
    }
  }

  const baseline =
    seed.revisions.find((r) => r.revisionId === seed.row.latestRevisionId) ??
    seed.revisions[seed.revisions.length - 1]
  const priceOrTaxChanged =
    content.salesPriceGross !== baseline?.salesPriceGross ||
    content.salesTaxRate !== baseline?.salesTaxRate

  if (priceOrTaxChanged) {
    return {
      kind: "REVIEW_POLICY_UNCONFIGURED",
      gateVersion: `pg-${seed.row.publicationId}-recalc`,
      submissionKind: "NORMAL",
      priceOrTaxChanged: true,
      blocker: {
        code: "REVIEW_POLICY_UNCONFIGURED",
        message:
          "销售价或销项税率发生变化，但复核政策尚未配置，不能提交发布。",
      },
    }
  }

  return {
    kind: "READY",
    gateVersion: `pg-${seed.row.publicationId}-ready`,
    submissionKind: "NORMAL",
    priceOrTaxChanged: false,
    policyVersion: "pol-1",
    reviewDisposition: "NOT_REQUIRED",
  }
}

export function submitW22PublishRevision(
  command: PublishRevisionCommand
): PublishRevisionResult {
  const existing = w22PublishIdempotency.get(command.requestId)
  if (existing) return existing

  if (command.forceUnknown) {
    const unknown: PublishRevisionResult = {
      status: "unknown",
      requestId: command.requestId,
      message:
        "发布请求结果未知。请使用同一请求编号查询，勿重复创建新版本。",
    }
    w22PublishIdempotency.set(command.requestId, unknown)
    w22PendingPublish.set(command.requestId, command)
    return unknown
  }

  const seed = baseOrOverride(command.publicationId)
  if (!seed) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "VALIDATION_FAILED",
      message: "发布对象不存在或无权访问。",
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }

  if (seed.objectVersion !== command.expectedObjectVersion) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "OBJECT_VERSION_CONFLICT",
      message: "数据版本已变更，请刷新后基于最新基线重新准备。",
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }

  if (seed.publishGate.gateVersion !== command.expectedPublishGateVersion) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "GATE_VERSION_MISMATCH",
      message: "发布门禁版本已变化，请刷新后重新核对。",
      publishGate: seed.publishGate,
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }

  const gate = recomputePublishGate(seed, command.content)
  if (gate.kind === "REVIEW_POLICY_UNCONFIGURED") {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "REVIEW_POLICY_UNCONFIGURED",
      message: gate.blocker.message,
      publishGate: gate,
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }
  if (gate.kind === "RECOVERY_RESPONSIBILITY_UNCONFIRMED") {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
      message: gate.blocker.message,
      publishGate: gate,
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }
  if (gate.kind !== "READY") {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "REVIEW_BLOCKED",
      message: "当前复核状态不允许提交发布。",
      publishGate: gate,
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }

  const fixedOffering = resolveFixedOffering(
    seed,
    command.content.supplierOfferingRevisionId
  )
  const validationMessage = validatePublishContent(seed, command, fixedOffering)
  if (validationMessage) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "VALIDATION_FAILED",
      message: validationMessage,
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }

  const revisionNo =
    Math.max(...seed.revisions.map((r) => r.revisionNo), 0) + 1
  const revisionId = `rev_sess_${++w22RevSeq}`
  const deliveryId = `dlv_sess_${++w22DlvSeq}`
  const operationId = `op_pub_${++w22OpSeq}`
  const committedAt = new Date().toISOString()

  const newRevision = {
    revisionId,
    revisionNo,
    skuRevisionId: command.content.skuRevisionId,
    supplierOfferingRevisionId: command.content.supplierOfferingRevisionId,
    fixedOffering: fixedOffering!,
    categoryId: command.content.categoryId,
    categoryLabel: `类目 ${command.content.categoryId}`,
    name: command.content.name,
    specification: command.content.specification,
    salesDescription: command.content.salesDescription,
    // 最小购买量来自运营输入，不从供应商 MOQ 自动复制
    minimumPurchaseQuantity: command.content.minimumPurchaseQuantity,
    // 销售价独立维护，供货价变化不自动写入
    salesPriceGross: command.content.salesPriceGross,
    salesTaxRate: command.content.salesTaxRate,
    baseUnitCode: command.content.baseUnitCode,
    salesRegion: [...command.content.salesRegion],
    salesRegionLabel: command.content.salesRegion.join("、"),
    saleStatus: command.content.saleStatus,
    saleStatusLabel: SALE_STATUS_LABEL[command.content.saleStatus],
    productCapabilities: command.content.productCapabilities,
    validFrom: command.content.validFrom,
    validTo: command.content.validTo,
    contentHash: `ch-sess-${revisionNo}`,
    media: command.content.media.map((m) => ({
      fileAssetId: m.fileAssetId,
      mediaRole: m.mediaRole as "MAIN" | "CAROUSEL" | "DETAIL",
      sortNo: m.sortNo,
      altText: m.altText,
      thumbnailUrl: "/placeholder-product.svg",
      securityScanStatus: "PASSED" as const,
    })),
    createdAt: committedAt,
    createdBy: "当前用户",
  }

  const newDelivery = {
    deliveryId,
    revisionId,
    revisionNo,
    targetMallId: seed.row.targetMallId,
    status: "PENDING_SEND" as const,
    statusLabel: DELIVERY_STATUS_LABEL.PENDING_SEND,
    statusTone: DELIVERY_STATUS_TONE.PENDING_SEND,
    attemptCount: 0,
  }

  const next: PublicationSeed = {
    ...seed,
    objectVersion: `ov-sess-${revisionNo}`,
    publishGate: gate,
    revisions: [...seed.revisions, newRevision],
    deliveries: [...seed.deliveries, newDelivery],
    row: {
      ...seed.row,
      publicationStatus: "PENDING_PUBLISH",
      publicationStatusLabel: PUBLICATION_STATUS_LABEL.PENDING_PUBLISH,
      publicationStatusTone: PUBLICATION_STATUS_TONE.PENDING_PUBLISH,
      latestRevisionId: revisionId,
      latestRevisionNo: revisionNo,
      hasPendingConfirmation: true,
      salesPriceGross: command.content.salesPriceGross,
      salesTaxRate: command.content.salesTaxRate,
      fixedOffering: newRevision.fixedOffering,
      latestDelivery: {
        deliveryId,
        status: "PENDING_SEND",
        statusLabel: DELIVERY_STATUS_LABEL.PENDING_SEND,
        statusTone: DELIVERY_STATUS_TONE.PENDING_SEND,
        attemptCount: 0,
      },
      updatedAt: committedAt,
    },
    allowedActions: ["PREPARE_REVISION", "QUERY_RESULT", "RETRY_DELIVERY"],
    actionBlockers: seed.actionBlockers,
  }

  w22Overrides.set(command.publicationId, next)

  const result: PublishRevisionResult = {
    status: "succeeded",
    operationId,
    publicationId: command.publicationId,
    revisionId,
    revisionNo,
    deliveryId,
    deliveryStatus: "PENDING_SEND",
    committedAt,
  }
  w22PublishIdempotency.set(command.requestId, result)
  return result
}

export function resolveW22PublishUnknown(
  command: ResolvePublishUnknownCommand
): PublishRevisionResult {
  const existing = w22PublishIdempotency.get(command.requestId)
  if (existing && existing.status !== "unknown") return existing

  if (command.settle) {
    const pending = w22PendingPublish.get(command.requestId)
    if (!pending) {
      return (
        existing ?? {
          status: "unknown",
          requestId: command.requestId,
          message: "未找到原请求，结果仍未知。",
        }
      )
    }
    w22PublishIdempotency.delete(command.requestId)
    w22PendingPublish.delete(command.requestId)
    return submitW22PublishRevision({ ...pending, forceUnknown: false })
  }

  return (
    existing ?? {
      status: "unknown",
      requestId: command.requestId,
      message: "结果仍未知，请稍后再次查询或进入异常处理。",
    }
  )
}

export function submitW22ManualPause(
  command: ManualPauseCommand
): ManualPauseResult {
  const seed = baseOrOverride(command.publicationId)
  if (!seed) {
    return { status: "blocked", code: "NOT_FOUND", message: "发布对象不存在。" }
  }
  if (seed.objectVersion !== command.expectedObjectVersion) {
    return {
      status: "blocked",
      code: "OBJECT_VERSION_CONFLICT",
      message: "数据版本已变更，请刷新后重试。",
    }
  }
  if (!seed.allowedActions.includes("PAUSE")) {
    return {
      status: "blocked",
      code: "ACTION_NOT_ALLOWED",
      message: "当前不允许人工暂停。",
    }
  }

  const revisionNo =
    Math.max(...seed.revisions.map((r) => r.revisionNo), 0) + 1
  const revisionId = `rev_pause_${++w22RevSeq}`
  const deliveryId = `dlv_pause_${++w22DlvSeq}`
  const committedAt = new Date().toISOString()
  const baseline =
    seed.revisions.find((r) => r.revisionId === seed.row.latestRevisionId) ??
    seed.revisions[seed.revisions.length - 1]

  if (!baseline) {
    return {
      status: "blocked",
      code: "VALIDATION_FAILED",
      message: "缺少基线修订。",
    }
  }

  const newRevision = {
    ...baseline,
    revisionId,
    revisionNo,
    saleStatus: "PAUSED" as const,
    saleStatusLabel: SALE_STATUS_LABEL.PAUSED,
    contentHash: `ch-pause-${revisionNo}`,
    createdAt: committedAt,
    createdBy: "当前用户",
    salesDescription: `${baseline.salesDescription}\n（人工暂停：${command.reason}）`,
  }

  const next: PublicationSeed = {
    ...seed,
    objectVersion: `ov-pause-${revisionNo}`,
    revisions: [...seed.revisions, newRevision],
    deliveries: [
      ...seed.deliveries,
      {
        deliveryId,
        revisionId,
        revisionNo,
        targetMallId: seed.row.targetMallId,
        status: "PENDING_SEND",
        statusLabel: DELIVERY_STATUS_LABEL.PENDING_SEND,
        statusTone: DELIVERY_STATUS_TONE.PENDING_SEND,
        attemptCount: 0,
      },
    ],
    row: {
      ...seed.row,
      publicationStatus: "PAUSED",
      publicationStatusLabel: PUBLICATION_STATUS_LABEL.PAUSED,
      publicationStatusTone: PUBLICATION_STATUS_TONE.PAUSED,
      latestRevisionId: revisionId,
      latestRevisionNo: revisionNo,
      hasPendingConfirmation: true,
      latestDelivery: {
        deliveryId,
        status: "PENDING_SEND",
        statusLabel: DELIVERY_STATUS_LABEL.PENDING_SEND,
        statusTone: DELIVERY_STATUS_TONE.PENDING_SEND,
        attemptCount: 0,
      },
      updatedAt: committedAt,
    },
  }
  w22Overrides.set(command.publicationId, next)
  return {
    status: "succeeded",
    revisionId,
    revisionNo,
    deliveryId,
    committedAt,
  }
}

export function submitW22RetryDelivery(
  command: RetryDeliveryCommand
): RetryDeliveryResult {
  const seed = baseOrOverride(command.publicationId)
  if (!seed) {
    return { status: "blocked", code: "NOT_FOUND", message: "发布对象不存在。" }
  }
  const delivery = seed.deliveries.find(
    (d) => d.deliveryId === command.deliveryId
  )
  if (!delivery) {
    return { status: "blocked", code: "NOT_FOUND", message: "投递记录不存在。" }
  }
  if (delivery.status === "SENDING" || delivery.status === "RETRYING") {
    return {
      status: "blocked",
      code: "ALREADY_IN_FLIGHT",
      message: "投递进行中，请勿重复重试。",
    }
  }
  if (delivery.status === "HANDOFF") {
    return {
      status: "blocked",
      code: "HANDOFF_REQUIRED",
      message: "已转人工，请进入接口错误处理。",
    }
  }
  if (delivery.status === "ACKED") {
    return {
      status: "blocked",
      code: "ALREADY_ACKED",
      message: "商城已确认，无需重试。",
    }
  }

  const attemptCount = delivery.attemptCount + 1
  const nextDeliveries = seed.deliveries.map((d) =>
    d.deliveryId === command.deliveryId
      ? {
          ...d,
          status: "RETRYING" as const,
          statusLabel: DELIVERY_STATUS_LABEL.RETRYING,
          statusTone: DELIVERY_STATUS_TONE.RETRYING,
          attemptCount,
          lastAttemptAt: new Date().toISOString(),
        }
      : d
  )
  const next: PublicationSeed = {
    ...seed,
    deliveries: nextDeliveries,
    row: {
      ...seed.row,
      latestDelivery: {
        deliveryId: delivery.deliveryId,
        status: "RETRYING",
        statusLabel: DELIVERY_STATUS_LABEL.RETRYING,
        statusTone: DELIVERY_STATUS_TONE.RETRYING,
        attemptCount,
        errorSummary: delivery.errorSummary,
      },
      updatedAt: new Date().toISOString(),
    },
  }
  w22Overrides.set(command.publicationId, next)
  return {
    status: "succeeded",
    deliveryId: delivery.deliveryId,
    attemptCount,
    deliveryStatus: "RETRYING",
  }
}

function unknownSafetyPause(
  command: SystemSafetyPauseTrigger,
  operationId: string
): SystemSafetyPauseOperationView {
  return {
    operationId,
    resultStatus: "UNKNOWN",
    cause: command.cause,
    sourceObjectType: command.sourceObjectType,
    sourceObjectId: command.sourceObjectId,
    sourceVersion: command.sourceVersion,
    subjectHash: command.subjectHash,
    originalIdempotencyKey: command.idempotencyKey,
    availabilityEffect: "FAIL_CLOSED_PENDING_RESULT",
  }
}

/**
 * 模拟服务端领域事件处理器：一次事件要么暂停全部发布对象，要么一个都不写入。
 * 页面不得直接构造该命令；测试或上游 mock API 可用它验证安全暂停闭环。
 */
export function triggerW22SystemSafetyPause(
  command: SystemSafetyPauseTrigger
): SystemSafetyPauseOperationView {
  const cached = w22SafetyPauseIdempotency.get(command.idempotencyKey)
  if (cached) return cached

  const operationId = `op_safety_${++w22OpSeq}`
  const publicationIds = [...new Set(command.affectedPublicationIds)]
  const seeds = publicationIds.map(baseOrOverride)
  const invalidCommand =
    !command.idempotencyKey.trim() ||
    !command.sourceObjectId.trim() ||
    !command.sourceVersion.trim() ||
    !command.subjectHash.trim() ||
    !command.occurredAt.trim() ||
    publicationIds.length === 0 ||
    seeds.some((seed) => seed == null) ||
    seeds.some(
      (seed) =>
        seed?.row.publicationStatus !== "SAFETY_PAUSED" &&
        seed?.revisions.length === 0
    )

  if (invalidCommand) {
    const unknown = unknownSafetyPause(command, operationId)
    w22SafetyPauseIdempotency.set(command.idempotencyKey, unknown)
    return unknown
  }

  const resolvedSeeds = seeds as PublicationSeed[]
  const alreadySafe = resolvedSeeds.every(
    (seed) => seed.row.publicationStatus === "SAFETY_PAUSED"
  )
  const prepared = resolvedSeeds.map((seed) => {
    const existingArtifact =
      seed.row.safetyPause?.resultStatus !== "UNKNOWN"
        ? seed.row.safetyPause?.affectedPublications.find(
            (affected) => affected.publicationId === seed.row.publicationId
          )
        : undefined
    if (seed.row.publicationStatus === "SAFETY_PAUSED") {
      return {
        seed,
        artifact:
          existingArtifact ??
          ({
            publicationId: seed.row.publicationId,
            pauseArtifactKind: "ACTION" as const,
            pauseActionId: `act_already_safe_${seed.row.publicationId}`,
            deliveryId:
              seed.row.latestDelivery?.deliveryId ??
              `dlv_already_safe_${seed.row.publicationId}`,
            outboxMessageId: `obx_already_safe_${seed.row.publicationId}`,
          } as const),
      }
    }

    const revisionNo =
      Math.max(...seed.revisions.map((revision) => revision.revisionNo), 0) + 1
    const revisionId = `rev_safety_${++w22RevSeq}`
    const deliveryId = `dlv_safety_${++w22DlvSeq}`
    return {
      seed,
      revisionNo,
      revisionId,
      deliveryId,
      outboxMessageId: `obx_safety_${w22DlvSeq}`,
      artifact: {
        publicationId: seed.row.publicationId,
        pauseArtifactKind: "REVISION" as const,
        pauseRevisionId: revisionId,
        deliveryId,
        outboxMessageId: `obx_safety_${w22DlvSeq}`,
      },
    }
  })

  const affectedPublications = prepared.map((item) => item.artifact) as [
    (typeof prepared)[number]["artifact"],
    ...(typeof prepared)[number]["artifact"][],
  ]
  const common = {
    operationId,
    resultStatus: alreadySafe ? ("ALREADY_SAFE" as const) : ("COMMITTED" as const),
    sourceObjectType: command.sourceObjectType,
    sourceObjectId: command.sourceObjectId,
    sourceVersion: command.sourceVersion,
    subjectHash: command.subjectHash,
    availabilityEffect: "PAUSED" as const,
    affectedPublications,
    committedAt: command.occurredAt,
  }

  let operation: SystemSafetyPauseOperationView
  if (command.cause === "SUPPLIER_STOPPED") {
    operation = {
      ...common,
      cause: command.cause,
      followUpWorkItem: {
        workItemId: `wi_safety_${w22OpSeq}`,
        workItemType: "BUSINESS_EXCEPTION" as const,
        businessObjectType: command.sourceObjectType,
        businessObjectId: command.sourceObjectId,
        subjectVersion: command.sourceVersion,
        subjectHash: command.subjectHash,
        handlerKey: "W21.supplierSupplierProduct.exception",
      },
    }
  } else if (
    command.cause === "ZERO_INVENTORY" ||
    command.cause === "SUPPLY_UNAVAILABLE" ||
    command.cause === "AVAILABILITY_STALE"
  ) {
    operation = {
      ...common,
      cause: command.cause,
      followUpBlocker: {
        code: "NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY" as const,
        message:
          "安全暂停已提交；当前政策不创建人工后续任务，来源恢复也不会自动上架。",
        evidenceReference: `ev-safety-${w22OpSeq}`,
      },
    }
  } else {
    operation = {
      ...common,
      cause: command.cause,
      followUpBlocker: {
        code: "NORMAL_REVIEW_WORK_ITEM_TYPE_UNREGISTERED" as const,
        message:
          "正常复核任务类型尚未登记；已安全暂停并保留 blocker，未伪造人工任务。",
        evidenceReference: `ev-safety-${w22OpSeq}`,
      },
    }
  }

  // operation 与全部写入内容都准备完成后再统一提交，模拟同一事务边界。
  for (const item of prepared) {
    if (item.seed.row.publicationStatus === "SAFETY_PAUSED") continue
    if (
      item.revisionNo == null ||
      !item.revisionId ||
      !item.deliveryId ||
      !item.outboxMessageId
    ) {
      continue
    }
    const baseline =
      item.seed.revisions.find(
        (revision) => revision.revisionId === item.seed.row.latestRevisionId
      ) ?? item.seed.revisions[item.seed.revisions.length - 1]
    const revision = {
      ...baseline,
      revisionId: item.revisionId,
      revisionNo: item.revisionNo,
      saleStatus: "PAUSED" as const,
      saleStatusLabel: SALE_STATUS_LABEL.PAUSED,
      contentHash: `ch-safety-${command.subjectHash}-${item.revisionNo}`,
      createdAt: command.occurredAt,
      createdBy: "系统",
    }
    const next: PublicationSeed = {
      ...item.seed,
      objectVersion: `ov-safety-${command.sourceVersion}-${item.revisionNo}`,
      publishGate: {
        kind: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
        gateVersion: `pg-safety-${command.sourceVersion}-${item.revisionNo}`,
        submissionKind: "RECOVERY",
        blocker: {
          code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
          message: "安全暂停后的恢复责任未确认，不能恢复上架。",
        },
      },
      revisions: [...item.seed.revisions, revision],
      deliveries: [
        ...item.seed.deliveries,
        {
          deliveryId: item.deliveryId,
          revisionId: item.revisionId,
          revisionNo: item.revisionNo,
          targetMallId: item.seed.row.targetMallId,
          status: "PENDING_SEND",
          statusLabel: DELIVERY_STATUS_LABEL.PENDING_SEND,
          statusTone: DELIVERY_STATUS_TONE.PENDING_SEND,
          attemptCount: 0,
        },
      ],
      allowedActions: ["QUERY_RESULT"],
      actionBlockers: [
        ...item.seed.actionBlockers.filter(
          (blocker) => blocker.action !== "PUBLISH"
        ),
        {
          action: "PUBLISH",
          code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
          message: "安全暂停后的恢复责任未确认，不能恢复上架。",
        },
      ],
      row: {
        ...item.seed.row,
        publicationStatus: "SAFETY_PAUSED",
        publicationStatusLabel: PUBLICATION_STATUS_LABEL.SAFETY_PAUSED,
        publicationStatusTone: PUBLICATION_STATUS_TONE.SAFETY_PAUSED,
        latestRevisionId: item.revisionId,
        latestRevisionNo: item.revisionNo,
        hasPendingConfirmation: true,
        safetyPause: operation,
        latestDelivery: {
          deliveryId: item.deliveryId,
          status: "PENDING_SEND",
          statusLabel: DELIVERY_STATUS_LABEL.PENDING_SEND,
          statusTone: DELIVERY_STATUS_TONE.PENDING_SEND,
          attemptCount: 0,
        },
        ownerLabel: "系统",
        updatedAt: command.occurredAt,
        allowedActions: ["QUERY_RESULT"],
        actionBlockers: [
          ...item.seed.row.actionBlockers.filter(
            (blocker) => blocker.action !== "PUBLISH"
          ),
          {
            action: "PUBLISH",
            code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
            message: "安全暂停后的恢复责任未确认，不能恢复上架。",
          },
        ],
      },
    }
    w22Overrides.set(item.seed.row.publicationId, next)
  }

  w22SafetyPauseIdempotency.set(command.idempotencyKey, operation)
  return operation
}
