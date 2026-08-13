import Link from "next/link"

import { WorkTaskItem, surfacePanelClassName } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { integrationStatusTone } from "../lib/presentation"
import type { IntegrationResolutionItemView } from "../types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

type IntegrationQueuePanelProps = Readonly<{
    items: readonly IntegrationResolutionItemView[]
    selectedId?: string
    onSelect: (item: IntegrationResolutionItemView) => void
}>

export function IntegrationQueuePanel({
    items,
    selectedId,
    onSelect,
}: IntegrationQueuePanelProps) {
    return (
        <Card size="sm" className={cn("min-h-[28rem]", surfacePanelClassName)}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>任务 / 差异队列</CardTitle>
                <CardDescription>
                    共 {items.length} 项 · 安全故障与结果未知优先
                </CardDescription>
            </CardHeader>
            <CardContent className="max-h-[70vh] space-y-2 overflow-y-auto pt-3">
                {items.map((item) => {
                    const detailHref =
                        item.identity.itemType === "ERROR_TASK"
                            ? `/governance/integration-errors/errors/${item.identity.id}`
                            : `/governance/integration-errors/differences/${item.identity.id}`
                    return (
                        <div
                            key={item.identity.id}
                            role="button"
                            tabIndex={0}
                            className={cn(
                                "w-full cursor-pointer rounded-xl text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-primary",
                                item.identity.id === selectedId
                                    ? "ring-2 ring-primary"
                                    : "hover:bg-muted/40",
                            )}
                            onClick={() => onSelect(item)}
                            onKeyDown={(event) => {
                                if (
                                    event.key === "Enter" ||
                                    event.key === " "
                                ) {
                                    event.preventDefault()
                                    onSelect(item)
                                }
                            }}
                        >
                            <WorkTaskItem
                                density="compact"
                                taskType={item.classification.label}
                                businessObject={item.businessObject.title}
                                counterparty={item.identity.number}
                                enteredAt={formatDateTime(
                                    item.createdAt,
                                    "default",
                                )}
                                enteredDateTime={item.createdAt}
                                dueAt={
                                    item.dueAt
                                        ? formatDateTime(item.dueAt, "default")
                                        : "—"
                                }
                                dueDateTime={item.dueAt}
                                responsibleParty={
                                    item.ownerUser ?? item.ownerRole
                                }
                                reason={item.classification.label}
                                impact={item.fundsImpactLabel}
                                status={{
                                    label: item.status.label,
                                    tone: integrationStatusTone(item),
                                }}
                                nextAction={
                                    <span className="flex flex-wrap items-center gap-1">
                                        <Badge variant="outline">
                                            {item.environmentLabel}
                                        </Badge>
                                        <Badge variant="outline">
                                            {item.classification.severityLabel}
                                        </Badge>
                                        <Link
                                            href={detailHref}
                                            className="text-xs text-primary underline-offset-2 hover:underline"
                                        >
                                            详情
                                        </Link>
                                    </span>
                                }
                            />
                        </div>
                    )
                })}
            </CardContent>
        </Card>
    )
}
