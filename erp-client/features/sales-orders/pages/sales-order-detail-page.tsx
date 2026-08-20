"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessFailureState,
    FormalActionResult,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SalesOrderMarginRiskHint } from "@/features/sales-orders/components/sales-order-margin-risk-hint"
import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import {
    SalesOrderDetailCommandDialogs,
    SalesOrderDetailSecondaryActions,
} from "@/features/sales-orders/components/sales-order-detail-command-dialogs"
import { SalesOrderEditableCenter } from "@/features/sales-orders/components/sales-order-detail-editable-center"
import {
    FocusTaskBanner,
    LifecycleRail,
    SalesOrderIdentityHeader,
} from "@/features/sales-orders/components/sales-order-detail-panels"
import { SalesOrderDetailTabs } from "@/features/sales-orders/components/sales-order-detail-tabs"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import {
    useSalesOrderDetailRejectionResolution,
    useSalesOrderDetailStartChange,
} from "@/features/sales-orders/hooks/use-sales-order-detail-commands"
import { useSalesOrderDetailModeGuard } from "@/features/sales-orders/hooks/use-sales-order-detail-mode-guard"
import { useSalesOrderDetailUrlState } from "@/features/sales-orders/hooks/use-sales-order-detail-url-state"
import { salesOrderMarginRiskHint } from "@/features/sales-orders/lib/sales-order-approval"
import { deriveSalesOrderDetailState } from "@/features/sales-orders/lib/sales-order-detail-derived"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { cn } from "@/lib/utils"

/**
 * 销售单详情。实物/卡券销售单与销售变更单走通用审批区。
 * SalesReturnCase 为 NO_APPROVAL，退货处理单不展示绑定卡、运行摘要或决定弹窗；
 * 待仓储验收 / 待采购处理 / 待财务处理是履约分工态，不是审批复核。
 */
export function SalesOrderDetailPage({
    salesOrderId,
    section,
}: {
    salesOrderId: string
    section?: string
}) {
    const query = useSalesOrderDetailQuery(salesOrderId)
    const {
        returnTo,
        fromWorkspace,
        pageMode,
        focusedWorkItemId,
        fromQueue,
        backHref,
        backLabel,
        replaceOrderHref,
        selectSection,
        enterRejectionEdit,
        leaveRejectionEdit,
    } = useSalesOrderDetailUrlState({ salesOrderId })
    const rejectionResolution = useSalesOrderDetailRejectionResolution()
    const startChangeCommand = useSalesOrderDetailStartChange()
    const focusedWorkItemQuery = useWorkItemDetailQuery(focusedWorkItemId)
    const focusedWorkItem = focusedWorkItemQuery.data
        ? mapWorkItemDto(focusedWorkItemQuery.data)
        : undefined

    const [voidOpen, setVoidOpen] = React.useState(false)
    const [lowMarginOpen, setLowMarginOpen] = React.useState(false)
    const [lowMarginReason, setLowMarginReason] = React.useState("")
    const [lowMarginEvidence, setLowMarginEvidence] = React.useState("")
    const [changeConfirmOpen, setChangeConfirmOpen] = React.useState(false)
    const [result, setResult] =
        React.useState<SalesOrderDetailActionResult | null>(null)

    const commandLedgerRef = React.useRef<{
        salesOrderId: string
        ledger: FormalCommandKeyLedger
    } | null>(null)
    if (commandLedgerRef.current?.salesOrderId !== salesOrderId) {
        commandLedgerRef.current = {
            salesOrderId,
            ledger: new FormalCommandKeyLedger(),
        }
    }
    const commandLedger = commandLedgerRef.current.ledger

    const order = query.data
    useSalesOrderDetailModeGuard({
        order,
        pageMode,
        replaceOrderHref,
    })

    if (query.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="销售单" description="正在加载详情…" />
                <div
                    className={cn(surfacePanelClassName, "h-72 animate-pulse")}
                    aria-busy="true"
                    aria-label="加载中"
                />
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <PageHeader title="销售单" />
                <BusinessFailureState
                    title="销售单加载失败"
                    error={query.error}
                    onRetry={() => {
                        void query.refetch()
                    }}
                />
            </PageScaffold>
        )
    }

    if (!order) {
        return (
            <PageScaffold>
                <PageHeader
                    title="销售单不存在"
                    description="未找到这张销售单。可能编号有误，或当前角色无权查看。"
                    actions={
                        <Button render={<Link href="/sales/orders" />}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const derived = deriveSalesOrderDetailState(order, {
        section,
        pageMode,
        fromWorkspace,
        returnTo,
    })
    const marginHint = salesOrderMarginRiskHint(order)

    if (derived.showEditor) {
        return (
            <SalesOrderEditableCenter
                order={order}
                backHref={backHref}
                backLabel={backLabel}
                fromQueue={fromQueue}
                fromWorkspace={fromWorkspace}
                result={result}
                onResult={setResult}
                showBackToDetail={derived.openRejection}
                onBackToDetail={leaveRejectionEdit}
                canVoidAfterRejection={derived.canVoid}
                commandLedger={commandLedger}
            />
        )
    }

    const primaryTaskAction =
        order.nature !== "physical_service" &&
        derived.openRejection &&
        derived.canResubmit ? (
            <Button type="button" size="sm" onClick={enterRejectionEdit}>
                改完再报
            </Button>
        ) : derived.actionableFocusTask &&
          !(
              order.nature !== "physical_service" &&
              derived.openRejection &&
              derived.navSection === "overview"
          ) ? (
            <Button
                type="button"
                size="sm"
                onClick={() => selectSection(derived.actionableFocusTask!.id)}
            >
                {derived.actionableFocusTask.actionLabel}
            </Button>
        ) : null

    return (
        <PageScaffold>
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
                        ]}
                    />
                }
            />

            {derived.focusTask ? (
                <FocusTaskBanner
                    order={order}
                    focusTask={derived.focusTask}
                    canActOnRejection={derived.canResubmit || derived.canVoid}
                    action={
                        derived.bannerJump ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    selectSection(derived.focusTask!.id)
                                }
                            >
                                {derived.focusTask.actionLabel}
                            </Button>
                        ) : undefined
                    }
                />
            ) : null}

            {result ? (
                <FormalActionResult
                    status={result.status}
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={[
                        {
                            label: "销售单",
                            value: order.documentNumber,
                        },
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

            {marginHint ? <SalesOrderMarginRiskHint hint={marginHint} /> : null}

            {section === "change-review" ? (
                <SalesChangeOrderApprovalSection
                    salesOrderId={order.id}
                    nature={order.nature}
                    changeOrder={order.activeChangeOrder ?? null}
                    workItemId={focusedWorkItem?.workItemId}
                    expectedTaskVersion={focusedWorkItem?.taskVersion}
                    workItemAllowedActions={focusedWorkItem?.allowedActions}
                    onResult={setResult}
                />
            ) : null}

            <SalesOrderIdentityHeader
                order={order}
                primaryAction={primaryTaskAction}
                secondaryActions={
                    <SalesOrderDetailSecondaryActions
                        order={order}
                        openRejection={derived.openRejection}
                        canRequestLowMargin={
                            derived.isCard && derived.canRequestLowMargin
                        }
                        canVoid={derived.canVoid}
                        canStartChange={derived.canStartChange}
                        changeBlocker={derived.changeBlocker}
                        changePending={startChangeCommand.isPending}
                        onOpenLowMargin={() => setLowMarginOpen(true)}
                        onOpenVoid={() => setVoidOpen(true)}
                        onOpenChangeConfirm={() => setChangeConfirmOpen(true)}
                    />
                }
            />

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <div className="border-b border-grid px-3 py-2 md:px-4">
                    <LifecycleRail order={order} />
                </div>

                <SalesOrderDetailTabs
                    order={order}
                    section={section}
                    navSection={derived.navSection}
                    visibleNav={derived.visibleNav}
                    canAccept={derived.canAccept}
                    acceptanceExpanded={derived.acceptanceExpanded}
                    showApproval={derived.showApproval}
                    selfReturn={derived.selfReturn}
                    workItemId={
                        section === "change-review"
                            ? undefined
                            : focusedWorkItem?.workItemId
                    }
                    expectedTaskVersion={
                        section === "change-review"
                            ? undefined
                            : focusedWorkItem?.taskVersion
                    }
                    workItemAllowedActions={
                        section === "change-review"
                            ? undefined
                            : focusedWorkItem?.allowedActions
                    }
                    onSelectSection={selectSection}
                    onApprovalResult={setResult}
                />
            </div>

            <SalesOrderDetailCommandDialogs
                order={order}
                voidOpen={voidOpen}
                onVoidOpenChange={setVoidOpen}
                voidPending={rejectionResolution.isPending}
                onVoidConfirm={(reason) =>
                    rejectionResolution.voidAfterRejection({
                        order,
                        commandLedger,
                        onResult: setResult,
                        reason,
                    })
                }
                lowMarginOpen={lowMarginOpen}
                onLowMarginOpenChange={setLowMarginOpen}
                lowMarginReason={lowMarginReason}
                onLowMarginReasonChange={setLowMarginReason}
                lowMarginEvidence={lowMarginEvidence}
                onLowMarginEvidenceChange={setLowMarginEvidence}
                lowMarginPending={rejectionResolution.isPending}
                onLowMarginConfirm={() =>
                    rejectionResolution.requestLowMargin({
                        order,
                        commandLedger,
                        onResult: setResult,
                        reason: lowMarginReason,
                        evidence: lowMarginEvidence,
                    })
                }
                changeConfirmOpen={changeConfirmOpen}
                onChangeConfirmOpenChange={setChangeConfirmOpen}
                onChangeConfirm={() =>
                    startChangeCommand.startChange({
                        order,
                        commandLedger,
                        onResult: setResult,
                    })
                }
            />
        </PageScaffold>
    )
}
