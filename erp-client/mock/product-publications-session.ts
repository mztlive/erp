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

const w22Overrides = new Map<string, PublicationSeed>()
const w22SessionCreated: PublicationSeed[] = []
const w22PublishIdempotency = new Map<string, PublishRevisionResult>()
const w22PendingPublish = new Map<string, PublishRevisionCommand>()
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
      message: "对象版本已变化，请刷新后基于最新基线重新准备。",
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

  if (!command.content.salesDescription.trim()) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "VALIDATION_FAILED",
      message: "销售说明为必填。",
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }
  if (Number(command.content.minimumPurchaseQuantity) <= 0) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "VALIDATION_FAILED",
      message: "最小购买量必须大于 0。",
    }
    w22PublishIdempotency.set(command.requestId, blocked)
    return blocked
  }
  const mainMedia = command.content.media.filter((m) => m.mediaRole === "MAIN")
  if (mainMedia.length !== 1 || !mainMedia[0]?.altText.trim()) {
    const blocked: PublishRevisionResult = {
      status: "blocked",
      code: "VALIDATION_FAILED",
      message: "必须提供恰好一张主图且含图片说明。",
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

  const baselineOffering =
    seed.revisions.find((r) => r.revisionId === seed.row.latestRevisionId)
      ?.fixedOffering ?? seed.row.fixedOffering

  const newRevision = {
    revisionId,
    revisionNo,
    skuRevisionId: command.content.skuRevisionId,
    supplierOfferingRevisionId: command.content.supplierOfferingRevisionId,
    fixedOffering: {
      ...baselineOffering,
      offeringRevisionId: command.content.supplierOfferingRevisionId,
    },
    categoryId: command.content.categoryId,
    categoryLabel:
      seed.revisions[seed.revisions.length - 1]?.categoryLabel ?? "未分类",
    name: command.content.name,
    specification: command.content.specification,
    salesDescription: command.content.salesDescription,
    // 最小购买量来自运营输入，不从供应商 MOQ 自动复制
    minimumPurchaseQuantity: command.content.minimumPurchaseQuantity,
    // 销售价独立维护，供货价变化不自动写入
    salesPriceGross: command.content.salesPriceGross,
    salesTaxRate: command.content.salesTaxRate,
    baseUnitCode: command.content.baseUnitCode,
    salesRegionLabel: command.content.salesRegionLabel,
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
      message: "对象版本已变化，请刷新后重试。",
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
