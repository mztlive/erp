"use client"

import {
    Item,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemTitle,
} from "@/components/ui/item"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    pendingFactsOf,
    summarizeLineDecisions,
    type AcceptanceBatchSelection,
} from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceSalesLineGroup } from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceRegisterLineNav({
    lines,
    activeLineId,
    selected,
    onSelect,
}: {
    lines: readonly AcceptanceSalesLineGroup[]
    activeLineId: string
    selected: AcceptanceBatchSelection
    onSelect: (salesOrderLineId: string) => void
}) {
    return (
        <nav
            className="flex max-h-40 w-full shrink-0 flex-col self-stretch overflow-y-auto border-b border-border bg-muted/40 md:max-h-none md:w-64 md:border-r md:border-b-0"
            aria-label="待验收明细"
        >
            <p className="px-4 py-3 text-xs font-medium text-muted-foreground">
                待验收明细
                <span className="num ml-1">{lines.length}</span>
            </p>
            <ItemGroup className="gap-2 px-3 pb-4" data-size="sm">
                {lines.map((line) => {
                    const pending = pendingFactsOf([line])
                    const active = line.salesOrderLineId === activeLineId
                    return (
                        <Item
                            key={line.salesOrderLineId}
                            variant={active ? "outline" : "default"}
                            size="sm"
                            render={
                                <button
                                    id={`sales-orders-acceptance-register-line-${toAutomationIdSegment(line.salesOrderLineId)}`}
                                    type="button"
                                    aria-label={`明细 ${line.lineNo}，${line.itemSnapshot}`}
                                />
                            }
                            className={cn(
                                "w-full cursor-pointer",
                                active
                                    ? "bg-background shadow-xs"
                                    : "hover:bg-background/70",
                            )}
                            aria-current={active ? "true" : undefined}
                            onClick={() => onSelect(line.salesOrderLineId)}
                        >
                            <ItemContent className="min-w-0">
                                <ItemTitle className="w-full max-w-full">
                                    {line.lineNo} · {line.itemSnapshot}
                                </ItemTitle>
                                <ItemDescription className="text-xs">
                                    {pending.length} 批 ·{" "}
                                    {summarizeLineDecisions(
                                        line.fulfillmentFacts,
                                        selected,
                                    )}
                                </ItemDescription>
                            </ItemContent>
                        </Item>
                    )
                })}
            </ItemGroup>
        </nav>
    )
}
