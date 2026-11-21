"use client"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspacePage,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCreateForm } from "@/features/sales-orders/components/sales-order-create-form"
import { SalesOrderIdentityHeader } from "@/features/sales-orders/components/sales-order-detail-panels"
import { useSalesOrderDraftResumeQuery } from "@/features/sales-orders/hooks/queries"
import type { FormalCommandKeyLedger } from "@/lib/formal-command"

export function SalesOrderEditableCenter({
    order,
    backHref,
    backLabel,
    fromQueue,
    fromWorkspace,
    commandLedger,
}: {
    order: SalesOrderDetailView
    backHref: string
    backLabel: string
    fromQueue: boolean
    fromWorkspace: string | null
    commandLedger: FormalCommandKeyLedger
}) {
    const resumeQuery = useSalesOrderDraftResumeQuery(order.id)

    return (
        <DetailWorkspacePage>
            <SalesOrderIdentityHeader
                identityOnly
                order={order}
                back={{
                    id: "sales-orders-detail-editable-back",
                    label: backLabel,
                    href: backHref,
                }}
                navigationMeta={
                    fromQueue
                        ? fromWorkspace === "W01" || fromWorkspace === "W09"
                            ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                            : fromWorkspace === "W08"
                              ? "从采购单打开 · 处理完可点返回，回到列表原位"
                              : "从工作台打开 · 处理完可点返回，回到列表原位"
                        : undefined
                }
            />

            <DetailWorkspaceBody>
                {resumeQuery.isPending ? (
                    <div
                        className="max-w-4xl space-y-4 py-6"
                        aria-busy="true"
                        aria-label="正在加载可编辑内容"
                    >
                        <div className="h-5 w-28 animate-pulse rounded bg-muted" />
                        <div className="grid gap-4 sm:grid-cols-3">
                            <div className="h-9 animate-pulse rounded-md bg-muted" />
                            <div className="h-9 animate-pulse rounded-md bg-muted" />
                            <div className="h-9 animate-pulse rounded-md bg-muted" />
                        </div>
                        <div className="h-44 animate-pulse rounded-md bg-muted/70" />
                    </div>
                ) : resumeQuery.isError || !resumeQuery.data ? (
                    <div className="max-w-3xl py-6">
                        <BusinessFailureState
                            title="可编辑内容加载失败"
                            error={resumeQuery.error}
                            action={
                                <Button
                                    id="sales-orders-detail-editable-center-resume-retry"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={() => {
                                        void resumeQuery.refetch()
                                    }}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </div>
                ) : (
                    <div className="flex min-w-0 flex-1 flex-col py-5">
                        <SalesOrderCreateForm
                            chrome="none"
                            purpose="draft"
                            initialDraft={resumeQuery.data}
                            initialNature={resumeQuery.data.nature}
                            initialContractId={resumeQuery.data.contractId}
                            commandLedger={commandLedger}
                        />
                    </div>
                )}
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
