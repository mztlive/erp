"use client"

import Link from "next/link"
import { XIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type {
    FormalSubmitResult,
    SupplierAccountsListView,
} from "@/features/supplier-payables/types"

export interface SupplierAccountsAlertsProps {
    fromWorkspace: string | undefined
    purchaseOrderId: string | undefined
    returnTo: string | undefined
    policy: SupplierAccountsListView["payablePriorityPolicy"]
}

export interface SupplierAccountsResultBannerProps {
    lastResult: FormalSubmitResult | null
    onDismiss: () => void
}

export function SupplierAccountsResultBanner({
    lastResult,
    onDismiss,
}: SupplierAccountsResultBannerProps) {
    if (!lastResult) return null
    return (
        <div className="relative">
            <FormalActionResult
                status={
                    lastResult.status === "succeeded"
                        ? "succeeded"
                        : lastResult.status === "unknown"
                          ? "unknown"
                          : lastResult.status === "blocked"
                            ? "blocked"
                            : "rejected"
                }
                title={lastResult.title}
                description={lastResult.description}
                reference={lastResult.reference ?? lastResult.operationId}
                facts={lastResult.facts}
                actions={
                    lastResult.returnTo && lastResult.status === "succeeded" ? (
                        <Button
                            id="supplier-payables-alerts-return-action"
                            type="button"
                            size="sm"
                            render={<Link href={lastResult.returnTo} />}
                        >
                            返回来源并重新校验先款条件
                        </Button>
                    ) : null
                }
            />
            <Button
                id="supplier-payables-alerts-dismiss"
                type="button"
                variant="ghost"
                size="icon-sm"
                className="absolute top-2 right-2"
                aria-label="收起结果"
                onClick={onDismiss}
            >
                <XIcon aria-hidden="true" />
            </Button>
        </div>
    )
}
