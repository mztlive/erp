"use client"

import {
    BusinessStatusBadge,
    DocumentSection,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export function PurchaseOrderDetailChangesSection({
    order,
    canChange,
    changeBlocker,
    onRequestChange,
}: {
    order: PurchaseOrderCenterView
    canChange: boolean
    changeBlocker:
        | PurchaseOrderCenterView["actionBlockers"][number]
        | undefined
    onRequestChange: () => void
}) {
    return (
        <DocumentSection title="变更与异常">
            {order.changes.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    暂无采购变更。生效后变化须走变更，不得在本版本直接覆写付款/发票/履约记录。
                </p>
            ) : (
                <ul className="space-y-2">
                    {order.changes.map((change) => (
                        <li
                            key={change.changeId}
                            className={cn(
                                surfaceInsetClassName,
                                "flex items-center justify-between px-3 py-2 text-sm",
                            )}
                        >
                            <span>
                                {change.label}
                                {change.baseRevisionNo != null
                                    ? ` · 基准 v${change.baseRevisionNo}`
                                    : ""}
                            </span>
                            <BusinessStatusBadge
                                context="list"
                                label={change.statusLabel}
                                tone={change.tone}
                            />
                        </li>
                    ))}
                </ul>
            )}
            <div className="mt-4 flex flex-wrap gap-2">
                {canChange ? (
                    <Button type="button" onClick={onRequestChange}>
                        发起采购变更
                    </Button>
                ) : (
                    <div className="space-y-1">
                        <Button type="button" disabled>
                            发起采购变更
                        </Button>
                        <p className="text-xs text-muted-foreground">
                            {changeBlocker?.message ??
                                "当前状态下不能发起变更，可先完成前置条件。"}
                        </p>
                    </div>
                )}
            </div>
        </DocumentSection>
    )
}
