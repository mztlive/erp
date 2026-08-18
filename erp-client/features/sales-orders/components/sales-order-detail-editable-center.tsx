"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, BanIcon, ShieldAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    FormalActionResult,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCreateForm } from "@/features/sales-orders/components/sales-order-create-form"
import { SalesOrderIdentityHeader } from "@/features/sales-orders/components/sales-order-detail-panels"
import { VoidSalesOrderDialog } from "@/features/sales-orders/components/void-sales-order-dialog"
import { useSalesOrderDetailRejectionResolution } from "@/features/sales-orders/hooks/use-sales-order-detail-commands"
import { useSalesOrderDraftResumeQuery } from "@/features/sales-orders/hooks/queries"
import { PROCUREMENT_REJECT_REASON_LABEL } from "@/features/sales-orders/lib/labels"
import {
    isOpenProcurementRejection,
    type SalesOrderDetailActionResult,
} from "@/features/sales-orders/lib/sales-order-detail-model"
import type { FormalCommandKeyLedger } from "@/lib/formal-command"
import { cn } from "@/lib/utils"

export function SalesOrderEditableCenter({
    order,
    backHref,
    backLabel,
    fromQueue,
    fromWorkspace,
    result,
    onResult,
    showBackToDetail = false,
    onBackToDetail,
    canVoidAfterRejection = false,
    commandLedger,
}: {
    order: SalesOrderDetailView
    backHref: string
    backLabel: string
    fromQueue: boolean
    fromWorkspace: string | null
    result: SalesOrderDetailActionResult | null
    onResult: (next: SalesOrderDetailActionResult) => void
    showBackToDetail?: boolean
    onBackToDetail?: () => void
    canVoidAfterRejection?: boolean
    commandLedger: FormalCommandKeyLedger
}) {
    const resumeQuery = useSalesOrderDraftResumeQuery(order.id)
    const { voidAfterRejection, isPending: voidPending } =
        useSalesOrderDetailRejectionResolution()
    const [voidOpen, setVoidOpen] = React.useState(false)
    const openRejection = isOpenProcurementRejection(order)
    const rejection = order.procurementRejection
    const canVoid = canVoidAfterRejection
    const reasonLabel = rejection
        ? (PROCUREMENT_REJECT_REASON_LABEL[rejection.rejectReasonCode] ??
          rejection.rejectReasonCode)
        : ""

    return (
        <PageScaffold className="pb-8">
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "orders", label: "销售单", href: "/sales/orders" },
                    {
                        id: "detail",
                        label: order.documentNumber,
                        current: true,
                    },
                ]}
                metadata={
                    fromQueue ? (
                        <span>
                            {fromWorkspace === "W09"
                                ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                                : fromWorkspace === "W08"
                                  ? "从采购单打开 · 处理完可点返回，回到列表原位"
                                  : "从工作台打开 · 处理完可点返回，回到列表原位"}
                        </span>
                    ) : undefined
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
                            ...(showBackToDetail && onBackToDetail
                                ? [
                                      {
                                          actionKey: "detail",
                                          label: "返回详情",
                                          variant: "outline" as const,
                                          onClick: onBackToDetail,
                                      },
                                  ]
                                : []),
                        ]}
                    />
                }
            />

            {openRejection && rejection ? (
                <Alert variant="warning" className="rounded-lg px-3 py-2">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle className="text-sm">采购未通过</AlertTitle>
                    <AlertDescription className="text-xs [&_p]:mb-0">
                        {[
                            reasonLabel,
                            rejection.rejectComment || null,
                            [rejection.rejectedByLabel, rejection.rejectedAt]
                                .filter(Boolean)
                                .join(" · ") || null,
                            `第 ${rejection.rejectedSubmissionNo} 次报给采购`,
                            "改完整单后再报，或点「作废」",
                        ]
                            .filter(Boolean)
                            .join(" · ")}
                    </AlertDescription>
                </Alert>
            ) : null}

            {result ? (
                <FormalActionResult
                    status={result.status}
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={[
                        { label: "销售单", value: order.documentNumber },
                        { label: "客户", value: order.customerName },
                        ...(result.nextResponsible
                            ? [
                                  {
                                      label: "下一步",
                                      value: result.nextResponsible,
                                  },
                              ]
                            : []),
                    ]}
                />
            ) : null}

            <SalesOrderIdentityHeader
                order={order}
                secondaryActions={
                    openRejection && canVoid ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => setVoidOpen(true)}
                        >
                            <BanIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            作废
                        </Button>
                    ) : undefined
                }
            />

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
                    purpose={openRejection ? "resubmit" : "draft"}
                    initialDraft={resumeQuery.data}
                    initialNature={resumeQuery.data.nature}
                    initialContractId={resumeQuery.data.contractId}
                    onResult={onResult}
                    commandLedger={commandLedger}
                />
            )}

            <VoidSalesOrderDialog
                open={voidOpen}
                onOpenChange={setVoidOpen}
                pending={voidPending}
                onConfirm={async (reason) => {
                    await voidAfterRejection({
                        order,
                        commandLedger,
                        onResult,
                        reason,
                    })
                }}
            />
        </PageScaffold>
    )
}
