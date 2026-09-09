"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"

import {
    BusinessFailureState,
    FormalActionResult,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import {
    SalesOrderDetailCommandDialogs,
    SalesOrderDetailSecondaryActions,
} from "@/features/sales-orders/components/sales-order-detail-command-dialogs"
import { SalesOrderEditableCenter } from "@/features/sales-orders/components/sales-order-detail-editable-center"
import { SalesOrderIdentityHeader } from "@/features/sales-orders/components/sales-order-detail-panels"
import { SalesOrderDetailSidebar } from "@/features/sales-orders/components/sales-order-detail-sidebar"
import { SalesOrderDetailTabs } from "@/features/sales-orders/components/sales-order-detail-tabs"
import {
    salesOrderKeys,
    useSalesOrderAcceptanceEligibilityQuery,
    useSalesOrderDetailQuery,
    useSalesChangeOrderDetailQuery,
} from "@/features/sales-orders/hooks/queries"
import { useSalesOrderDetailStartChange } from "@/features/sales-orders/hooks/use-sales-order-detail-commands"
import { useSalesOrderDetailUrlState } from "@/features/sales-orders/hooks/use-sales-order-detail-url-state"
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
    const query = useSalesOrderDetailQuery(salesOrderId, true)
    const eligibilityQuery = useSalesOrderAcceptanceEligibilityQuery(
        salesOrderId,
        Boolean(
            query.data &&
            query.data.nature === "physical_service" &&
            query.data.allowedActions.includes("REGISTER_ACCEPTANCE"),
        ),
    )
    const {
        returnTo,
        fromWorkspace,
        focusedWorkItemId,
        focusedChangeOrderId,
        fromQueue,
        backHref,
        backLabel,
        selectSection: selectUrlSection,
    } = useSalesOrderDetailUrlState({ salesOrderId })
    const startChangeCommand = useSalesOrderDetailStartChange()
    const focusedWorkItemQuery = useWorkItemDetailQuery(focusedWorkItemId)
    const focusedWorkItem = focusedWorkItemQuery.data
        ? mapWorkItemDto(focusedWorkItemQuery.data)
        : undefined
    const changeOrderId =
        section === "change-review"
            ? focusedChangeOrderId ||
              (focusedWorkItem?.businessObjectType === "sales_change_order"
                  ? focusedWorkItem.businessObjectId
                  : "")
            : ""
    const changeQuery = useSalesChangeOrderDetailQuery(
        salesOrderId,
        changeOrderId,
        query.data?.nature,
    )
    const selectSection = React.useCallback(
        (
            next: Parameters<typeof selectUrlSection>[0],
            extras?: { mode?: "register" },
        ) =>
            selectUrlSection(
                next,
                focusedWorkItem?.workItemType ===
                    "CUSTOMER_ACCEPTANCE_REGISTRATION",
                extras,
            ),
        [focusedWorkItem?.workItemType, selectUrlSection],
    )

    const [changeConfirmOpen, setChangeConfirmOpen] = React.useState(false)
    const [result, setResult] =
        React.useState<SalesOrderDetailActionResult | null>(null)

    // 审批决定/命令成功后单据状态会变（如审批中→已生效、改单中→新版本），
    // 统一刷新详情、列表与客户验收缓存，避免关联区继续显示旧状态。
    const queryClient = useQueryClient()
    const refreshOrderDetail = React.useCallback(() => {
        void queryClient.invalidateQueries({
            queryKey: salesOrderKeys.all,
        })
    }, [queryClient])
    const handleActionResult = React.useCallback(
        (next: SalesOrderDetailActionResult) => {
            if (next.status === "succeeded" || next.status === "unknown") {
                refreshOrderDetail()
            }
            setResult(next)
        },
        [refreshOrderDetail],
    )

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
                        <Button
                            id="sales-orders-detail-not-found-back"
                            render={<Link href="/sales/orders" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const derived = deriveSalesOrderDetailState(order, {
        section,
        fromWorkspace,
        returnTo,
        hasEligibleAcceptance: eligibilityQuery.data === true,
    })
    if (derived.showEditor) {
        return (
            <SalesOrderEditableCenter
                order={order}
                backHref={backHref}
                backLabel={backLabel}
                fromQueue={fromQueue}
                fromWorkspace={fromWorkspace}
                commandLedger={commandLedger}
            />
        )
    }

    return (
        <PageScaffold density="compact">
            <SalesOrderIdentityHeader
                back={{
                    id: "sales-orders-detail-back",
                    label: backLabel,
                    href: backHref,
                }}
                navigationMeta={
                    fromQueue
                        ? fromWorkspace === "W01" || fromWorkspace === "W09"
                            ? "从履约处理打开 · 查阅后可点返回，回到列表原位"
                            : fromWorkspace === "W08"
                              ? "从采购单打开 · 查阅后可点返回，回到列表原位"
                              : "从工作台打开 · 查阅后可点返回，回到列表原位"
                        : undefined
                }
                order={order}
                identityOnly
                secondaryActions={
                    <SalesOrderDetailSecondaryActions
                        order={order}
                        canStartChange={derived.canStartChange}
                        changeBlocker={derived.changeBlocker}
                        changePending={startChangeCommand.isPending}
                        onOpenChangeConfirm={() => setChangeConfirmOpen(true)}
                        onApprovalResult={handleActionResult}
                    />
                }
            />

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

            {section === "change-review" &&
            changeOrderId &&
            changeQuery.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在加载变更单…
                </p>
            ) : section === "change-review" &&
              changeOrderId &&
              changeQuery.isError ? (
                <BusinessFailureState
                    title="变更单读取失败"
                    description="请重新打开该变更单，当前销售单的其他变更不会替代此次内容。"
                />
            ) : section === "change-review" ? (
                <SalesChangeOrderApprovalSection
                    readonlyApproval
                    salesOrderId={order.id}
                    nature={order.nature}
                    changeOrder={
                        changeOrderId
                            ? (changeQuery.data ?? null)
                            : (order.activeChangeOrder ?? null)
                    }
                    workItemId={focusedWorkItem?.workItemId}
                    expectedTaskVersion={focusedWorkItem?.taskVersion}
                    workItemAllowedActions={focusedWorkItem?.allowedActions}
                    onResult={handleActionResult}
                />
            ) : null}

            <SalesOrderDetailTabs
                order={order}

                section={section}
                navSection={derived.navSection}
                visibleNav={derived.visibleNav}
                canAccept={derived.canAccept}
                onSelectSection={selectSection}
                onApprovalResult={handleActionResult}
                sidebar={
                    <SalesOrderDetailSidebar
                        hideFinanceSummary={derived.navSection === "receivable"}
                        order={order}
                        focusTask={derived.focusTask}
                        action={
                            derived.focusTask ? (
                                <Button
                                    id={`sales-orders-detail-banner-${derived.focusTask.id}`}
                                    type="button"
                                    variant="outline"
                                    onClick={() =>
                                        selectSection(derived.focusTask!.id)
                                    }
                                >
                                    {derived.focusTask.id === "approval"
                                        ? "查看审批"
                                        : derived.focusTask.id === "acceptance"
                                          ? "查看验收"
                                          : "查看版本"}
                                </Button>
                            ) : null
                        }
                    />
                }
            />

            <SalesOrderDetailCommandDialogs
                order={order}
                changeConfirmOpen={changeConfirmOpen}
                onChangeConfirmOpenChange={setChangeConfirmOpen}
                onChangeConfirm={() =>
                    startChangeCommand.startChange({
                        order,
                        commandLedger,
                        onResult: handleActionResult,
                    })
                }
            />
        </PageScaffold>
    )
}
