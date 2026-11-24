"use client"

import { useState } from "react"
import { ArrowUpRightIcon, ChevronRightIcon, PackageIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    QuickPreviewSheet,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SheetClose, SheetTrigger } from "@/components/ui/sheet"
import { DeliveryPackingDetails } from "@/features/fulfillment-operations/components/delivery-packing-details"
import type { SalesOrderLineItem } from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    formatOccurredAt,
    isPositiveQty,
    qtyWithUnit,
    type AcceptanceLineProgress,
} from "../lib/acceptance-model"
import {
    FULFILLMENT_TYPE_LABEL,
    type AcceptanceEligibleFact,
} from "../lib/acceptance-types"

/** 销售行交付批次轻预览抽屉：承接仓发、代发、电子交付与服务履约及包裹物流详情。 */
export function SalesOrderLineBatchesSheet({
    line,
    progress,
    canAccept,
    onStartAcceptance,
}: {
    line: SalesOrderLineItem
    progress?: AcceptanceLineProgress
    canAccept?: boolean
    onStartAcceptance?: () => void
}) {
    const [open, setOpen] = useState(false)
    const facts = progress?.fulfillmentFacts ?? []
    const prefix = `sales-order-line-${toAutomationIdSegment(line.id)}-batches`

    if (!progress || facts.length === 0) {
        return (
            <span className="text-xs text-muted-foreground">
                {progress ? "暂无批次" : "—"}
            </span>
        )
    }

    const pendingCount = progress.pendingFacts.length
    const hasPending = pendingCount > 0
    const status = lineSummaryStatus(progress)

    return (
        <QuickPreviewSheet
            idPrefix={`${prefix}-sheet`}
            open={open}
            onOpenChange={setOpen}
            size="preview"
            identity={`交付批次 · 第 ${progress.lineNo} 行`}
            title={line.name}
            description={
                line.sku && line.sku !== "无规格" ? line.sku : undefined
            }
            summary={
                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                    <BusinessStatusBadge
                        context="preview"
                        tone={status.tone}
                        label={status.label}
                    />
                    <span className="num text-xs text-muted-foreground">
                        已交{" "}
                        {qtyWithUnit(
                            progress.deliveredQuantity,
                            line.unitName ?? "",
                        )}
                        {" / "}
                        {qtyWithUnit(
                            progress.requiredQuantity,
                            line.unitName ?? "",
                        )}
                        {" · 已验 "}
                        {qtyWithUnit(
                            progress.acceptedQuantity,
                            line.unitName ?? "",
                        )}
                    </span>
                </div>
            }
            trigger={
                <SheetTrigger
                    id={`${prefix}-trigger`}
                    aria-label={`查看${line.name}的交付批次，共${facts.length}批`}
                    render={
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.inlineLinkButton}
                        />
                    }
                >
                    <span className="num">{facts.length} 批</span>
                    <ChevronRightIcon
                        aria-hidden="true"
                        data-icon="inline-end"
                    />
                </SheetTrigger>
            }
            footer={
                <>
                    <SheetClose
                        id={`${prefix}-footer-close`}
                        render={
                            <Button type="button" variant="outline" size="sm" />
                        }
                    >
                        关闭
                    </SheetClose>
                    {canAccept && hasPending && onStartAcceptance ? (
                        <Button
                            id={`${prefix}-go-accept`}
                            type="button"
                            size="sm"
                            onClick={() => {
                                setOpen(false)
                                onStartAcceptance()
                            }}
                        >
                            去登记验收
                            <ArrowUpRightIcon
                                aria-hidden="true"
                                data-icon="inline-end"
                            />
                        </Button>
                    ) : null}
                </>
            }
        >
            <ol className="divide-y divide-border/60">
                {facts.map((fact) => (
                    <BatchItem
                        key={fact.fulfillmentLineId}
                        fact={fact}
                        salesOrderLineId={line.id}
                    />
                ))}
            </ol>
        </QuickPreviewSheet>
    )
}

function BatchItem({
    fact,
    salesOrderLineId,
}: {
    fact: AcceptanceEligibleFact
    salesOrderLineId: string
}) {
    const trackingEntries = (fact.trackingEntries ?? []).filter(
        (entry) => entry.salesOrderLineId === salesOrderLineId,
    )
    const pending = isPositiveQty(fact.eligibleQuantity)
    const accepted = isPositiveQty(fact.netAcceptedAllocatedQuantity)
    const batchStatus = pending
        ? {
              tone: "info" as const,
              label: `待验 ${qtyWithUnit(fact.eligibleQuantity, fact.unitName)}`,
          }
        : accepted
          ? { tone: "success" as const, label: "已验收" }
          : { tone: "warning" as const, label: "待核对" }

    return (
        <li className="space-y-2.5 py-4 first:pt-0 last:pb-0">
            <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0 space-y-0.5">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <Badge variant="secondary" className="font-normal">
                            {FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]}
                        </Badge>
                        <span className="num text-body-compact font-medium text-foreground">
                            {fact.fulfillmentNo}
                        </span>
                    </div>
                    {fact.occurredAt ? (
                        <p className="num text-xs text-muted-foreground">
                            {formatOccurredAt(fact.occurredAt)}
                        </p>
                    ) : null}
                </div>
                <BusinessStatusBadge
                    context="preview"
                    tone={batchStatus.tone}
                    label={batchStatus.label}
                />
            </div>

            <dl className="grid grid-cols-3 gap-3 rounded-lg bg-muted/35 px-3 py-2 text-xs">
                <div className="space-y-0.5">
                    <dt className="text-muted-foreground">本批交付</dt>
                    <dd className="num font-medium text-foreground">
                        {qtyWithUnit(fact.netSuccessfulQuantity, fact.unitName)}
                    </dd>
                </div>
                <div className="space-y-0.5">
                    <dt className="text-muted-foreground">已通过验收</dt>
                    <dd className="num font-medium text-foreground">
                        {qtyWithUnit(
                            fact.netAcceptedAllocatedQuantity,
                            fact.unitName,
                        )}
                    </dd>
                </div>
                <div className="space-y-0.5">
                    <dt className="text-muted-foreground">待验收</dt>
                    <dd
                        className={
                            pending
                                ? "num font-medium text-primary"
                                : "num text-muted-foreground"
                        }
                    >
                        {qtyWithUnit(fact.eligibleQuantity, fact.unitName)}
                    </dd>
                </div>
            </dl>

            {trackingEntries.length > 0 ? (
                <div className="space-y-1 text-xs">
                    <p className="inline-flex items-center gap-1 font-medium text-muted-foreground">
                        <PackageIcon aria-hidden="true" className="size-3.5" />
                        物流单号
                    </p>
                    <ul className="space-y-0.5 text-foreground">
                        {trackingEntries.map((entry) => (
                            <li
                                key={`${entry.salesOrderLineId}:${entry.carrier ?? ""}:${entry.trackingNo}`}
                                className="num"
                            >
                                {entry.carrier ? `${entry.carrier} · ` : ""}
                                <span className="font-mono">
                                    {entry.trackingNo}
                                </span>
                            </li>
                        ))}
                    </ul>
                </div>
            ) : null}

            {fact.deliveryId ? (
                <DeliveryPackingDetails
                    deliveryId={fact.deliveryId}
                    fulfillmentLineId={fact.fulfillmentLineId}
                    idPrefix={`sales-delivery-${toAutomationIdSegment(fact.fulfillmentLineId)}`}
                />
            ) : null}
        </li>
    )
}

function lineSummaryStatus(progress: AcceptanceLineProgress): {
    tone: "success" | "info" | "warning" | "neutral"
    label: string
} {
    if (progress.stuckKind === "done") {
        return { tone: "success", label: "已验收" }
    }
    if (progress.stuckKind === "accept") {
        return { tone: "info", label: "待验收" }
    }
    if (progress.stuckKind === "exception") {
        return { tone: "warning", label: "待处理" }
    }
    return { tone: "neutral", label: "待交付" }
}
