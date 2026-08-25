"use client"

import Link from "next/link"
import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessFailureState,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCreateForm } from "@/features/sales-orders/components/sales-order-create-form"
import { SalesOrderIdentityHeader } from "@/features/sales-orders/components/sales-order-detail-panels"
import { useSalesOrderDraftResumeQuery } from "@/features/sales-orders/hooks/queries"
import type { FormalCommandKeyLedger } from "@/lib/formal-command"
import { cn } from "@/lib/utils"

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
        <PageScaffold className="pb-8">
            <PageHeader
                variant="object-chrome"
                metadata={
                    <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span className="text-xl font-semibold tracking-tight text-foreground">
                            销售单
                        </span>
                        {fromQueue ? (
                            <span>
                                {fromWorkspace === "W09"
                                    ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                                    : fromWorkspace === "W08"
                                      ? "从采购单打开 · 处理完可点返回，回到列表原位"
                                      : "从工作台打开 · 处理完可点返回，回到列表原位"}
                            </span>
                        ) : null}
                    </span>
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "back",
                                label: backLabel,
                                icon: ArrowLeftIcon,
                                variant: "outline",
                                render: <Link href={backHref} />,
                            },
                        ]}
                    />
                }
            />

            <SalesOrderIdentityHeader order={order} />

            {resumeQuery.isPending ? (
                <div
                    className={cn(surfacePanelClassName, "h-72 animate-pulse")}
                    aria-busy="true"
                    aria-label="正在加载可编辑内容"
                />
            ) : resumeQuery.isError || !resumeQuery.data ? (
                <BusinessFailureState
                    title="可编辑内容加载失败"
                    error={resumeQuery.error}
                    onRetry={() => {
                        void resumeQuery.refetch()
                    }}
                />
            ) : (
                <SalesOrderCreateForm
                    chrome="none"
                    purpose="draft"
                    initialDraft={resumeQuery.data}
                    initialNature={resumeQuery.data.nature}
                    initialContractId={resumeQuery.data.contractId}
                    commandLedger={commandLedger}
                />
            )}
        </PageScaffold>
    )
}
