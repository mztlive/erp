"use client"

import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { VIEW_LABEL } from "@/features/inventory/types"
import type { InventoryView } from "@/features/inventory/types"

interface LedgerViewTabsProps {
    view: InventoryView
    onViewChange: (nextView: InventoryView) => void
}

export function LedgerViewTabs({ view, onViewChange }: LedgerViewTabsProps) {
    return (
        <Tabs
            value={view}
            onValueChange={(v) => {
                onViewChange(v as InventoryView)
            }}
        >
            <TabsList variant="line" className="w-full justify-start">
                {(
                    [
                        "balance",
                        "movement",
                        "reservation",
                        "adjustment",
                    ] as const
                ).map((v) => (
                    <TabsTrigger key={v} value={v}>
                        {VIEW_LABEL[v]}
                    </TabsTrigger>
                ))}
            </TabsList>
        </Tabs>
    )
}
