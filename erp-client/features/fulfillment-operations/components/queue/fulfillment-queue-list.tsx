"use client"

import * as React from "react"

import { WorkTaskItem } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { FulfillmentOperation } from "@/features/fulfillment-operations/types"
import { OPERATION_TYPE_LABEL } from "@/features/fulfillment-operations/types"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"

export function FulfillmentQueueList({
    operations,
    currentIndex,
    position,
    total,
    page,
    totalPages,
    onSelect,
    onPageChange,
}: {
    operations: readonly FulfillmentOperation[]
    currentIndex: number
    position: number
    total: number
    page: number
    totalPages: number
    onSelect: (operationId: string) => void
    onPageChange: (page: number) => void
}) {
    const containerRef = React.useRef<HTMLDivElement | null>(null)
    const itemRefs = React.useRef(new Map<string, HTMLButtonElement>())

    // 切换单据后把当前项滚入可见区，让操作员始终看得到自己的位置
    React.useEffect(() => {
        const current = operations[currentIndex]
        if (!current) return
        const el = itemRefs.current.get(current.operationId)
        el?.scrollIntoView({ block: "nearest", behavior: "smooth" })
    }, [currentIndex, operations])

    return (
        <Card size="sm" className="min-w-0 self-start">
            <CardHeader className="border-b">
                <CardTitle>待处理单据</CardTitle>
                <CardDescription>
                    第 {position} 条，共 {total} 条
                </CardDescription>
            </CardHeader>
            <CardContent
                ref={containerRef}
                className="max-h-[min(36rem,70vh)] space-y-2 overflow-y-auto"
            >
                {operations.map((item, index) => (
                    <button
                        key={item.operationId}
                        id={`fulfillment-operations-queue-item-${toAutomationIdSegment(item.operationId)}`}
                        type="button"
                        ref={(el) => {
                            if (el) itemRefs.current.set(item.operationId, el)
                        }}
                        className={cn(
                            "w-full text-left",
                            index === currentIndex &&
                                "rounded-lg ring-2 ring-primary",
                        )}
                        onClick={() => onSelect(item.operationId)}
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
                                label: item.overdue
                                    ? "已超期"
                                    : item.statusLabel,
                                tone: item.overdue
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
                                {compactFixed(
                                    sumFixed(
                                        item.lines.map(
                                            (line) =>
                                                line.remainingQuantity || "0",
                                        ),
                                        {
                                            maxScale: 6,
                                            outputScale: 6,
                                        },
                                    ),
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
            {totalPages > 1 ? (
                <CardFooter className="justify-between gap-2 border-t">
                    <Button
                        id="fulfillment-operations-queue-prev-page"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={page <= 1}
                        onClick={() => onPageChange(page - 1)}
                    >
                        上一页
                    </Button>
                    <span
                        className="text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        第 {page} / {totalPages} 页
                    </span>
                    <Button
                        id="fulfillment-operations-queue-next-page"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={page >= totalPages}
                        onClick={() => onPageChange(page + 1)}
                    >
                        下一页
                    </Button>
                </CardFooter>
            ) : null}
        </Card>
    )
}
