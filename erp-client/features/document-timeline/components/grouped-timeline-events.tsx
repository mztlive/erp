"use client"

import { ChevronDownIcon, ClipboardListIcon } from "lucide-react"
import { StatusBadge } from "@/components/ui/status-badge"
import { approvalResultText } from "@/lib/ui-text"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { TimelineEvent } from "../types"
import { TimelineEvents } from "./timeline-events"

/** 按单据收拢当前已加载的记录，展开后保留原始时间线、结果与单据入口。 */
export function GroupedTimelineEvents({
    events,
    idPrefix,
    plainApprovalReasons,
}: {
    events: TimelineEvent[]
    idPrefix: string
    plainApprovalReasons?: boolean
}) {
    if (!events.length)
        return <TimelineEvents events={events} idPrefix={idPrefix} />
    const groups = new Map<string, TimelineEvent[]>()
    for (const event of events) {
        const key = JSON.stringify([event.document_type, event.document_id])
        groups.set(key, [...(groups.get(key) ?? []), event])
    }
    return (
        <div className="divide-y overflow-hidden rounded-lg border border-border/70">
            {Array.from(groups, ([key, items]) => {
                const latest = items[0]
                return (
                    <details key={key} className="group">
                        <summary
                            id={`${idPrefix}-document-${toAutomationIdSegment(key)}`}
                            className="flex cursor-pointer list-none items-start gap-3 px-4 py-4 hover:bg-muted/40 [&::-webkit-details-marker]:hidden"
                        >
                            <ClipboardListIcon
                                className="mt-0.5 size-5 shrink-0 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <span className="w-24 shrink-0 text-body-compact text-muted-foreground">
                                {new Date(
                                    latest.occurred_at * 1000,
                                ).toLocaleDateString("zh-CN")}
                            </span>
                            <span className="min-w-0 flex-1 space-y-1">
                                <span className="flex flex-wrap items-center gap-2 text-body-compact font-medium">
                                    {latest.title}
                                    {latest.approval ? (
                                        <StatusBadge
                                            tone={
                                                latest.approval.result ===
                                                "APPROVED"
                                                    ? "success"
                                                    : latest.approval.result ===
                                                        "REJECTED"
                                                      ? "destructive"
                                                      : "neutral"
                                            }
                                            label={
                                                latest.approval.result ===
                                                "APPROVED"
                                                    ? approvalResultText.approved
                                                    : latest.approval.result ===
                                                        "REJECTED"
                                                      ? approvalResultText.rejected
                                                      : approvalResultText.cancelled
                                            }
                                        />
                                    ) : null}
                                </span>
                                <span className="block break-words text-xs text-muted-foreground">
                                    {latest.document_label} ·{" "}
                                    {latest.document_number} · 已加载{" "}
                                    {items.length} 条记录
                                </span>
                            </span>
                            <ChevronDownIcon
                                className="mt-1 size-4 shrink-0 text-muted-foreground group-open:rotate-180"
                                aria-hidden="true"
                            />
                        </summary>
                        <div className="border-t bg-muted/20 px-5 py-4">
                            <TimelineEvents
                                events={items}
                                idPrefix={idPrefix}
                                plainApprovalReasons={plainApprovalReasons}
                            />
                        </div>
                    </details>
                )
            })}
        </div>
    )
}
