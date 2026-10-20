"use client"
import { Badge } from "@/components/ui/badge"
import { MoneyValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { TimelineEvent } from "../types"

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
        <ol aria-label="业务时间线" className="space-y-0">
            {events.map((event) => {
                const isCurrentDocument =
                    currentDocument?.type === event.document_type &&
                    currentDocument.id === event.document_id
                return (
                    <li
                        key={event.id}
                        className="relative ml-1 border-l border-border pb-6 pl-5 last:border-transparent last:pb-0"
                    >
                        <span
                            aria-hidden="true"
                            className="absolute -left-1 top-1.5 size-2 rounded-full bg-primary/70"
                        />
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                            <time
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
                            {!timeOnly && <span>{event.actor_name}</span>}
                            {event.after_submission && (
                                <Badge variant="warning">提交后发生</Badge>
                            )}
                        </div>
                        <p className="mt-1.5 text-sm font-medium">
                            {timeOnly && event.actor_name && (
                                <span className="mr-2">{event.actor_name}</span>
                            )}
                            {event.title}
                        </p>
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
                            <ul className="mt-2 space-y-1 text-xs leading-5 text-muted-foreground">
                                {event.details.map((detail, index) => (
                                    <li
                                        key={`${event.id}-${index}`}
                                        className="whitespace-pre-wrap break-words"
                                    >
                                        {detail}
                                    </li>
                                ))}
                            </ul>
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
                    </li>
                )
            })}
        </ol>
    )
}
