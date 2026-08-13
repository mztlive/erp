"use client"

import * as React from "react"

import { WorkTaskItem } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import type { FulfillmentTask } from "@/features/fulfillment-operations/types"
import { OPERATION_TYPE_LABEL } from "@/features/fulfillment-operations/types"

export function FulfillmentQueueList({
    tasks,
    currentIndex,
    position,
    total,
    onSelect,
}: {
    tasks: readonly FulfillmentTask[]
    currentIndex: number
    position: number
    total: number
    onSelect: (workItemId: string) => void
}) {
    const containerRef = React.useRef<HTMLDivElement | null>(null)
    const itemRefs = React.useRef(new Map<string, HTMLButtonElement>())

    // 切换任务后把当前项滚入可见区，让操作员始终看得到自己的位置
    React.useEffect(() => {
        const current = tasks[currentIndex]
        if (!current) return
        const el = itemRefs.current.get(current.workItemId)
        el?.scrollIntoView({ block: "nearest", behavior: "smooth" })
    }, [currentIndex, tasks])

    return (
        <Card size="sm" className="min-w-0 self-start">
            <CardHeader className="border-b">
                <CardTitle>待办</CardTitle>
                <CardDescription>
                    第 {position} 条，共 {total} 条
                </CardDescription>
            </CardHeader>
            <CardContent
                ref={containerRef}
                className="max-h-[min(36rem,70vh)] space-y-2 overflow-y-auto"
            >
                {tasks.map((item, index) => (
                    <button
                        key={item.workItemId}
                        type="button"
                        ref={(el) => {
                            if (el) itemRefs.current.set(item.workItemId, el)
                        }}
                        className={cn(
                            "w-full text-left",
                            index === currentIndex &&
                                "rounded-lg ring-2 ring-primary",
                        )}
                        onClick={() => onSelect(item.workItemId)}
                    >
                        <WorkTaskItem
                            density="compact"
                            taskType={OPERATION_TYPE_LABEL[item.operationType]}
                            businessObject={`${item.source.salesOrderNo}${
                                item.source.purchaseNo
                                    ? ` · ${item.source.purchaseNo}`
                                    : ""
                            }`}
                            counterparty={item.source.customerLabel}
                            enteredAt={item.dueLabel}
                            enteredDateTime={item.dueAt}
                            dueAt={item.dueLabel}
                            dueDateTime={item.dueAt}
                            responsibleParty={item.responsibleLabel}
                            reason={item.summary}
                            impact={item.impact}
                            status={{
                                label: item.held
                                    ? "已跳过"
                                    : item.overdue
                                      ? "已超期"
                                      : item.statusLabel,
                                tone: item.held
                                    ? "warning"
                                    : item.overdue
                                      ? "destructive"
                                      : item.statusTone,
                            }}
                        />
                        {/* 类型已由 taskType 显示，这里不再重复；改为交代明细行数 */}
                        <div className="mt-1 flex flex-wrap gap-1 px-1 pb-1">
                            <Badge
                                variant="secondary"
                                className="font-normal num"
                            >
                                待处理{" "}
                                {item.lines.reduce(
                                    (sum, line) =>
                                        sum +
                                        Number(line.remainingQuantity || 0),
                                    0,
                                )}
                                {item.lines[0]?.unitCode ?? ""}
                            </Badge>
                            {item.lines.length > 1 ? (
                                <Badge
                                    variant="outline"
                                    className="font-normal"
                                >
                                    另 {item.lines.length - 1} 行明细
                                </Badge>
                            ) : null}
                        </div>
                    </button>
                ))}
            </CardContent>
        </Card>
    )
}
