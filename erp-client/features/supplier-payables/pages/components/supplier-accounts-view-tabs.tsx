"use client"

import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    SUPPLIER_ACCOUNT_VIEWS,
    VIEW_LABEL,
    type SupplierAccountsView,
} from "@/features/supplier-payables/types"
import { parseView } from "@/features/supplier-payables/lib/url-state"

export interface SupplierAccountsViewTabsProps {
    view: SupplierAccountsView
    onViewChange: (view: SupplierAccountsView) => void
}

/**
 * 供应商往来工作视图：独立胶囊 Tabs，不与搜索/筛选挤在同一行。
 */
export function SupplierAccountsViewTabs({
    view,
    onViewChange,
}: SupplierAccountsViewTabsProps) {
    return (
        <Tabs
            value={view}
            onValueChange={(next) => {
                if (!next) return
                const parsed = parseView(next)
                if (parsed === view) return
                onViewChange(parsed)
            }}
        >
            <TabsList variant="solid" aria-label="供应商往来工作视图">
                {SUPPLIER_ACCOUNT_VIEWS.map((item) => (
                    <TabsTrigger key={item} value={item}>
                        {VIEW_LABEL[item]}
                    </TabsTrigger>
                ))}
            </TabsList>
        </Tabs>
    )
}
