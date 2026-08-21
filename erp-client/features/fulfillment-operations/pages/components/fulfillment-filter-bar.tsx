"use client"

import { surfacePanelClassName } from "@/components/business"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { FulfillmentQueueToolbar } from "@/features/fulfillment-operations/components/queue/fulfillment-queue-toolbar"
import type { DueFilter, GateFilter } from "@/features/fulfillment-operations/lib/filters"
import type {
    FulfillmentOperation,
    FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import {
    OPERATION_TYPE_SHORT,
    SLUG_TO_TYPE,
    TYPE_SLUG,
} from "@/features/fulfillment-operations/types"
import { cn } from "@/lib/utils"

export type FulfillmentFilterBarProps = {
    activeTypeSlug: string
    visibleTypes: readonly FulfillmentOperationType[]
    onTypeChange: (next: FulfillmentOperationType | "all") => void
    q: string | undefined
    warehouseId: string | undefined
    due: DueFilter | undefined
    gate: GateFilter | undefined
    salesOrderId: string | undefined
    purchaseOrderId: string | undefined
    operations: readonly FulfillmentOperation[]
    autoNext: boolean
    showAutoNext: boolean
    onPatch: (patch: Record<string, string | null | undefined>) => void
    onClearAllFilters: () => void
    onAutoNextChange: (next: boolean) => void
}

/** M3 sticky 处理面：第 0 层范围/类型 + 第 1/2 层 ListToolbar（ui-filter-design §2.3）。 */
export function FulfillmentFilterBar({
    activeTypeSlug,
    visibleTypes,
    onTypeChange,
    q,
    warehouseId,
    due,
    gate,
    salesOrderId,
    purchaseOrderId,
    operations,
    autoNext,
    showAutoNext,
    onPatch,
    onClearAllFilters,
    onAutoNextChange,
}: FulfillmentFilterBarProps) {
    const warehouseLabel = operations.find(
        (t) => t.source.warehouseId === warehouseId,
    )?.source.warehouseLabel
    return (
        <div
            className={cn(
                surfacePanelClassName,
                "sticky top-0 z-10 space-y-2.5 px-3 py-2.5",
            )}
        >
            <div className="flex flex-wrap items-center gap-2">
                <ToggleGroup
                    value={[
                        activeTypeSlug === "all" ? "all" : activeTypeSlug,
                    ]}
                    onValueChange={(values) => {
                        const next = values[0]
                        if (!next) return
                        if (next === "all") onTypeChange("all")
                        else {
                            const t = SLUG_TO_TYPE[next]
                            if (t) onTypeChange(t)
                        }
                    }}
                    variant="outline"
                    spacing={0}
                    className="w-fit flex-wrap"
                    aria-label="作业类型"
                >
                    <ToggleGroupItem value="all">全部</ToggleGroupItem>
                    {visibleTypes.map((t) => (
                        <ToggleGroupItem key={t} value={TYPE_SLUG[t]}>
                            {OPERATION_TYPE_SHORT[t]}
                        </ToggleGroupItem>
                    ))}
                </ToggleGroup>
            </div>

            <FulfillmentQueueToolbar
                q={q}
                warehouseId={warehouseId}
                due={due}
                gate={gate}
                salesOrderId={salesOrderId}
                purchaseOrderId={purchaseOrderId}
                salesOrderNo={
                    operations.find(
                        (t) => t.source.salesOrderId === salesOrderId,
                    )?.source.salesOrderNo
                }
                purchaseNo={
                    operations.find(
                        (t) => t.source.purchaseOrderId === purchaseOrderId,
                    )?.source.purchaseNo
                }
                warehouseLabel={warehouseLabel}
                autoNext={autoNext}
                showAutoNext={showAutoNext}
                type={activeTypeSlug}
                onPatch={onPatch}
                onClearAllFilters={onClearAllFilters}
                onAutoNextChange={onAutoNextChange}
            />
        </div>
    )
}
