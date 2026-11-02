"use client"
import { BanIcon, CheckIcon, XIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Timeline,
    TimelineItem,
    TimelineMarker,
    TimelineTitle,
} from "@/components/ui/timeline"
import { MoneyValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { approvalResultText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import type { TimelineEvent } from "../types"

const approvalPresentation = {
    APPROVED: {
        label: approvalResultText.approved,
        tone: "success",
        icon: CheckIcon,
        marker: "border-success-border bg-success-soft text-success-soft-foreground",
    },
    REJECTED: {
        label: approvalResultText.rejected,
        tone: "destructive",
        icon: XIcon,
        marker: "border-destructive-border bg-destructive-soft text-destructive-soft-foreground",
    },
    CANCELLED: {
        label: approvalResultText.cancelled,
        tone: "neutral",
        icon: BanIcon,
        marker: "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
    },
} as const

export function TimelineEvents({
    events,
    idPrefix,
    groupByDate = false,
    currentDocument,
}: {
    events: TimelineEvent[]
    idPrefix: string
    groupByDate?: boolean
    currentDocument?: { type: string; id: string }
}) {
    if (events.length === 0)
        return (
            <p className="py-5 text-sm text-muted-foreground">
                当前条件下暂无可查看的动作记录。
            </p>
        )
    if (groupByDate) {
        const groups = new Map<string, TimelineEvent[]>()
        for (const event of events) {
            const date = new Date(event.occurred_at * 1000).toLocaleDateString(
                "zh-CN",
            )
            const group = groups.get(date) ?? []
            group.push(event)
            groups.set(date, group)
        }
        return (
            <div className="space-y-6">
                {Array.from(groups, ([date, items]) => (
                    <section key={date} aria-label={date} className="space-y-3">
                        <h3 className="text-xs font-medium text-muted-foreground">
                            {date}
                        </h3>
                        <TimelineEventList
                            events={items}
                            idPrefix={idPrefix}
                            timeOnly
                            currentDocument={currentDocument}
                        />
                    </section>
                ))}
            </div>
        )
    }
    return (
        <TimelineEventList
            events={events}
            idPrefix={idPrefix}
            currentDocument={currentDocument}
        />
    )
}

function TimelineEventList({
    events,
    idPrefix,
    timeOnly = false,
    currentDocument,
}: {
    events: TimelineEvent[]
    idPrefix: string
    timeOnly?: boolean
    currentDocument?: { type: string; id: string }
}) {
    return (
        <Timeline aria-label="业务时间线">
            {events.map((event) => {
                const isCurrentDocument =
                    currentDocument?.type === event.document_type &&
                    currentDocument.id === event.document_id
                const approval = event.approval
                const presentation = approval
                    ? approvalPresentation[approval.result]
                    : undefined
                const Icon = presentation?.icon
                const rejected = approval?.result === "REJECTED"
                return (
                    <TimelineItem key={event.id} className="min-w-0 pb-6">
                        <TimelineMarker
                            aria-hidden="true"
                            className={presentation?.marker}
                        >
                            {Icon ? (
                                <Icon className="size-3.5" />
                            ) : (
                                <span className="size-1.5 rounded-full bg-muted-foreground" />
                            )}
                        </TimelineMarker>
                        <TimelineTitle className="flex min-h-6 min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                            <span className="min-w-0 break-words">
                                {approval
                                    ? (approval.node_name ?? "审批流程")
                                    : event.title}
                            </span>
                            {presentation && (
                                <StatusBadge
                                    tone={presentation.tone}
                                    icon={presentation.icon}
                                    label={presentation.label}
                                />
                            )}
                        </TimelineTitle>
                        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                            <time
                                className="num"
                                dateTime={new Date(
                                    event.occurred_at * 1000,
                                ).toISOString()}
                            >
                                {timeOnly
                                    ? new Date(
                                          event.occurred_at * 1000,
                                      ).toLocaleTimeString("zh-CN", {
                                          hour12: false,
                                      })
                                    : new Date(
                                          event.occurred_at * 1000,
                                      ).toLocaleString("zh-CN", {
                                          hour12: false,
                                      })}
                            </time>
                            {event.actor_name && (
                                <span>{event.actor_name}</span>
                            )}
                            {approval?.round_no != null && (
                                <span>第 {approval.round_no} 轮</span>
                            )}
                            {event.after_submission && (
                                <Badge variant="warning">提交后发生</Badge>
                            )}
                        </div>
                        {!isCurrentDocument && (
                            <p className="mt-1 text-xs text-muted-foreground">
                                {event.document_label}
                                {event.document_number
                                    ? ` · ${event.document_number}`
                                    : " · 未记录单号"}
                            </p>
                        )}
                        {event.amount != null && (
                            <p className="mt-2 text-sm">
                                {event.amount_label}：
                                <MoneyValue value={event.amount} />
                            </p>
                        )}
                        {event.business_date && (
                            <p className="mt-1 text-xs text-muted-foreground">
                                业务日期：{event.business_date}
                            </p>
                        )}
                        {event.details.length > 0 && (
                            <div
                                className={cn(
                                    "mt-2 text-xs leading-5",
                                    rejected
                                        ? "rounded-lg border border-destructive-border bg-destructive-soft px-3 py-2 text-destructive-soft-foreground"
                                        : "text-muted-foreground",
                                )}
                            >
                                {rejected && (
                                    <p className="mb-1 font-medium">
                                        {approvalResultText.rejectionReason}
                                    </p>
                                )}
                                <ul className="space-y-1">
                                    {event.details.map((detail, index) => (
                                        <li
                                            key={`${event.id}-${index}`}
                                            className="whitespace-pre-wrap break-words"
                                        >
                                            {detail}
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        )}
                        {event.href && !isCurrentDocument && (
                            <a
                                id={`${idPrefix}-open-${toAutomationIdSegment(event.id)}`}
                                href={event.href}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="mt-2 inline-block text-xs text-primary underline-offset-4 hover:underline"
                            >
                                打开{event.document_label}
                            </a>
                        )}
                    </TimelineItem>
                )
            })}
        </Timeline>
    )
}
