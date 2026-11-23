"use client"

import { useState } from "react"
import { ArrowUpRightIcon, ChevronRightIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { approvalResultText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import type { TimelineCategory, TimelineEvent } from "../types"
import { TimelineEvents } from "./timeline-events"

const CATEGORY_LABELS: Record<TimelineCategory, string> = {
    sales: "销售",
    approval: "审批",
    procurement: "采购",
    fulfillment: "履约",
    receivable: "客户票款",
    payable: "供应商票款",
    returns: "退货与调整",
}

const PLACEHOLDER_ACTORS = new Set(["未记录姓名", "未记录操作人", "审批流程"])

function realActorName(name?: string | null): string | undefined {
    const trimmed = name?.trim()
    return trimmed && !PLACEHOLDER_ACTORS.has(trimmed) ? trimmed : undefined
}

function resolveDocumentLabel(items: readonly TimelineEvent[]): string {
    const latest = items[0]
    if (latest.document_type === "invoice") {
        const direction =
            items.find(
                (item) =>
                    item.category === "receivable" ||
                    item.category === "payable",
            )?.category ?? latest.category
        if (direction === "receivable") return "销项发票"
        if (direction === "payable") return "进项发票"
    }
    return latest.document_label
}

function resolveBusinessCategory(items: readonly TimelineEvent[]): string {
    const nonApproval = items.find((item) => item.category !== "approval")
    return CATEGORY_LABELS[(nonApproval ?? items[0]).category]
}

function resolveApprovalBadge(latest: TimelineEvent): {
    tone: "success" | "destructive" | "neutral"
    label: string
} | null {
    const approval = latest.approval
    if (!approval) return null
    const tone =
        approval.result === "APPROVED"
            ? "success"
            : approval.result === "REJECTED"
              ? "destructive"
              : "neutral"
    if (approval.node_name) {
        return {
            tone,
            label:
                approval.result === "APPROVED"
                    ? approvalResultText.approved
                    : approval.result === "REJECTED"
                      ? approvalResultText.rejected
                      : approvalResultText.cancelled,
        }
    }
    const isNodeAction = latest.title.includes("节点")
    if (approval.result === "APPROVED") {
        return { tone, label: isNodeAction ? "节点通过" : "审批通过" }
    }
    if (approval.result === "REJECTED") {
        return { tone, label: isNodeAction ? "节点驳回" : "审批驳回" }
    }
    return {
        tone,
        label: latest.title.includes("受阻") ? "受阻取消" : "审批已取消",
    }
}

function resolveRejectionReason(
    latest: TimelineEvent,
    items: readonly TimelineEvent[],
): string | undefined {
    if (latest.approval?.result !== "REJECTED") return undefined
    const direct = latest.details[0]?.trim()
    if (direct) return direct
    const roundNo = latest.approval.round_no
    const matched = items.find(
        (item) =>
            item.approval?.result === "REJECTED" &&
            (roundNo == null || item.approval.round_no === roundNo) &&
            item.details.some((detail) => detail.trim().length > 0),
    )
    return matched?.details[0]?.trim()
}

function isSameDocument(
    currentDocument: { type: string; id: string } | undefined,
    event: TimelineEvent,
): boolean {
    if (!currentDocument || currentDocument.id !== event.document_id) {
        return false
    }
    return (
        currentDocument.type === event.document_type ||
        (currentDocument.type === "sales_order" &&
            event.document_type === "voucher_sales_order")
    )
}

/** 按单据收拢当前已加载的记录，采用纯白通栏细线分隔，展开后展示单据内时间线。 */
export function GroupedTimelineEvents({
    events,
    idPrefix,
    plainApprovalReasons,
    hasMore = false,
    currentDocument,
}: {
    events: TimelineEvent[]
    idPrefix: string
    plainApprovalReasons?: boolean
    hasMore?: boolean
    currentDocument?: { type: string; id: string }
}) {
    if (!events.length) {
        return (
            <TimelineEvents
                events={events}
                idPrefix={idPrefix}
                currentDocument={currentDocument}
            />
        )
    }

    const groups = new Map<string, TimelineEvent[]>()
    for (const event of events) {
        const key = JSON.stringify([event.document_type, event.document_id])
        groups.set(key, [...(groups.get(key) ?? []), event])
    }

    return (
        <div className="divide-y divide-border/60 border-y border-border/60">
            {Array.from(groups, ([groupKey, items]) => (
                <DocumentTimelineGroup
                    key={groupKey}
                    groupKey={groupKey}
                    items={items}
                    idPrefix={idPrefix}
                    plainApprovalReasons={plainApprovalReasons}
                    hasMore={hasMore}
                    currentDocument={currentDocument}
                />
            ))}
        </div>
    )
}

function DocumentTimelineGroup({
    groupKey,
    items,
    idPrefix,
    plainApprovalReasons,
    hasMore,
    currentDocument,
}: {
    groupKey: string
    items: TimelineEvent[]
    idPrefix: string
    plainApprovalReasons?: boolean
    hasMore: boolean
    currentDocument?: { type: string; id: string }
}) {
    const [expanded, setExpanded] = useState(false)
    const latest = items[0]
    const segment = toAutomationIdSegment(groupKey)
    const triggerId = `${idPrefix}-document-${segment}`
    const panelId = `${idPrefix}-document-panel-${segment}`

    const docLabel = resolveDocumentLabel(items)
    const docNumber = latest.document_number.trim() || "未记录单号"
    const categoryLabel = resolveBusinessCategory(items)
    const approvalBadge = resolveApprovalBadge(latest)
    const rejectionReason = resolveRejectionReason(latest, items)

    const amountEvents = items.filter((item) => item.amount != null)
    const latestAmountEvent = amountEvents[0]
    const amountPrefix =
        latestAmountEvent?.amount_label === "本次冲减金额"
            ? amountEvents.length > 1
                ? "最近冲减"
                : "冲减本单"
            : amountEvents.length > 1
              ? "最近归属"
              : "归属本单"

    const isCurrentDoc = isSameDocument(currentDocument, latest)
    const documentHref = !isCurrentDoc
        ? (items.find((item) => item.href)?.href ?? null)
        : null

    const occurredIso = new Date(latest.occurred_at * 1000).toISOString()
    const occurredLabel = formatDateTime(occurredIso, "full")

    const latestActor = realActorName(latest.actor_name)
    const isInstanceApprovalEnd =
        Boolean(latest.approval) && !latest.approval?.node_name
    const headlineAction =
        latest.approval?.node_name ??
        (!isInstanceApprovalEnd ? latest.title : null)

    const countLabel = hasMore
        ? `已加载 ${items.length} 条`
        : `${items.length} 条记录`

    const roundLabel =
        latest.approval?.round_no != null
            ? `第 ${latest.approval.round_no} 轮`
            : undefined

    const inlineDetail =
        !rejectionReason && !isInstanceApprovalEnd
            ? latest.details[0]?.trim()
            : undefined

    return (
        <div>
            <div className="relative -mx-2 rounded-md px-2 transition-colors hover:bg-muted/30">
                <div className="flex items-start gap-2.5 py-3">
                    <ChevronRightIcon
                        aria-hidden="true"
                        className={cn(
                            "mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform duration-150",
                            expanded && "rotate-90 text-foreground",
                        )}
                    />
                    <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                                <button
                                    type="button"
                                    id={triggerId}
                                    aria-expanded={expanded}
                                    aria-controls={panelId}
                                    onClick={() =>
                                        setExpanded((current) => !current)
                                    }
                                    className="cursor-pointer rounded-sm text-left font-medium text-foreground before:absolute before:inset-0 before:rounded-md focus-visible:outline-none focus-visible:before:ring-2 focus-visible:before:ring-ring"
                                >
                                    {docLabel}
                                    {headlineAction ? (
                                        <span className="font-normal text-muted-foreground">
                                            {" · "}
                                            <span className="font-medium text-foreground">
                                                {headlineAction}
                                            </span>
                                        </span>
                                    ) : null}
                                </button>
                                {approvalBadge ? (
                                    <StatusBadge
                                        tone={approvalBadge.tone}
                                        label={approvalBadge.label}
                                    />
                                ) : null}
                            </div>
                            <div className="ml-auto flex shrink-0 items-baseline gap-3">
                                {latestAmountEvent?.amount != null ? (
                                    <span className="inline-flex items-baseline gap-1 text-xs text-muted-foreground">
                                        <span>{amountPrefix}</span>
                                        <MoneyValue
                                            value={latestAmountEvent.amount}
                                            display="exact"
                                            className="text-body-compact"
                                        />
                                    </span>
                                ) : null}
                                <time
                                    dateTime={occurredIso}
                                    className="num whitespace-nowrap text-xs text-muted-foreground"
                                >
                                    {occurredLabel}
                                </time>
                            </div>
                        </div>

                        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-xs text-muted-foreground">
                            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
                                <span>{categoryLabel}</span>
                                <span
                                    aria-hidden="true"
                                    className="text-muted-foreground/40"
                                >
                                    ·
                                </span>
                                <span className="num break-all">
                                    {docNumber}
                                </span>
                                {roundLabel ? (
                                    <>
                                        <span
                                            aria-hidden="true"
                                            className="text-muted-foreground/40"
                                        >
                                            ·
                                        </span>
                                        <span className="num">
                                            {roundLabel}
                                        </span>
                                    </>
                                ) : null}
                                {latestActor ? (
                                    <>
                                        <span
                                            aria-hidden="true"
                                            className="text-muted-foreground/40"
                                        >
                                            ·
                                        </span>
                                        <span>{latestActor}</span>
                                    </>
                                ) : null}
                                {rejectionReason ? (
                                    <>
                                        <span
                                            aria-hidden="true"
                                            className="text-muted-foreground/40"
                                        >
                                            ·
                                        </span>
                                        <span className="line-clamp-1 text-destructive-soft-foreground">
                                            驳回原因：{rejectionReason}
                                        </span>
                                    </>
                                ) : inlineDetail ? (
                                    <>
                                        <span
                                            aria-hidden="true"
                                            className="text-muted-foreground/40"
                                        >
                                            ·
                                        </span>
                                        <span className="line-clamp-1">
                                            {inlineDetail}
                                        </span>
                                    </>
                                ) : null}
                                <span
                                    aria-hidden="true"
                                    className="text-muted-foreground/40"
                                >
                                    ·
                                </span>
                                <span className="num">{countLabel}</span>
                            </div>

                            {documentHref ? (
                                <a
                                    id={`${idPrefix}-document-open-${segment}`}
                                    href={documentHref}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    aria-label={`打开${docLabel} ${docNumber}`}
                                    className="relative z-10 inline-flex shrink-0 items-center gap-0.5 rounded-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                >
                                    打开{docLabel}
                                    <ArrowUpRightIcon
                                        className="size-3"
                                        aria-hidden="true"
                                    />
                                </a>
                            ) : null}
                        </div>
                    </div>
                </div>
            </div>

            {expanded ? (
                <div id={panelId} className="pb-4 pl-6.5 pt-1">
                    <TimelineEvents
                        events={items}
                        idPrefix={idPrefix}
                        currentDocument={{
                            type: latest.document_type,
                            id: latest.document_id,
                        }}
                        plainApprovalReasons={plainApprovalReasons}
                    />
                </div>
            ) : null}
        </div>
    )
}
