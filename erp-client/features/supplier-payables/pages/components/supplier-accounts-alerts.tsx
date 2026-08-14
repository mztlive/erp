"use client"

import Link from "next/link"
import { XIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"
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

export function SupplierAccountsAlerts({
    fromWorkspace,
    purchaseOrderId,
    returnTo,
    policy,
}: SupplierAccountsAlertsProps) {
    return (
        <>
            {(fromWorkspace || purchaseOrderId) && (
                <Alert>
                    <AlertTitle>跨页面进入</AlertTitle>
                    <AlertDescription>
                        {fromWorkspace
                            ? `来源 ${workspaceLabel(fromWorkspace as WorkspaceId)}`
                            : null}
                        {purchaseOrderId
                            ? ` · 采购单 ${purchaseOrderId}`
                            : null}
                        。完成付款核销后请返回来源页重新校验先款条件；未核销付款不满足先款要求。
                        {returnTo ? (
                            <>
                                {" "}
                                <Link className="underline" href={returnTo}>
                                    返回来源
                                </Link>
                            </>
                        ) : null}
                    </AlertDescription>
                </Alert>
            )}

            {policy.state !== "AVAILABLE" ? (
                <Alert>
                    <AlertTitle>混合自动分配不可用</AlertTitle>
                    <AlertDescription>
                        {policy.blockerMessage}
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
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
                    lastResult.returnTo &&
                    lastResult.status === "succeeded" ? (
                        <Button
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
