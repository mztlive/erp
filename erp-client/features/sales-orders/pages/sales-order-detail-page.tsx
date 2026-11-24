"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    FormalActionResult,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import { SalesChangeOrderEditDialog } from "@/features/sales-orders/components/sales-change-order-edit-dialog"
import {
    SalesOrderDetailSecondaryActions,
    SalesOrderStartChangeAction,
} from "@/features/sales-orders/components/sales-order-detail-command-dialogs"
import { SalesOrderEditableCenter } from "@/features/sales-orders/components/sales-order-detail-editable-center"
import {
    SalesOrderAmountSummary,
    SalesOrderIdentityHeader,
} from "@/features/sales-orders/components/sales-order-detail-panels"
import { SalesOrderDetailSidebar } from "@/features/sales-orders/components/sales-order-detail-sidebar"
import {
    SalesOrderDetailSectionContent,
    SalesOrderDetailSectionTabs,
} from "@/features/sales-orders/components/sales-order-detail-tabs"
import { SalesOrderProcurementAlert } from "@/features/sales-orders/components/sales-order-procurement-alert"
import { SalesOrderRelatedProblems } from "@/features/sales-orders/components/sales-order-related-problems"
import {
    salesOrderKeys,
    useSalesChangeOrderDetailQuery,
    useSalesOrderDeliveryProgressQuery,
    useSalesOrderDetailQuery,
} from "@/features/sales-orders/hooks/queries"
import { useSalesOrderDetailStartChange } from "@/features/sales-orders/hooks/use-sales-order-detail-commands"
import { useSalesOrderDetailUrlState } from "@/features/sales-orders/hooks/use-sales-order-detail-url-state"
import { deriveSalesOrderDetailState } from "@/features/sales-orders/lib/sales-order-detail-derived"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { salesOrderSectionGroup } from "@/features/sales-orders/lib/sales-order-detail-navigation"
import { canSupplementSalesOrderContract } from "@/features/sales-orders/lib/sales-order-detail-permissions"
import type { SalesChangeOrderSummary } from "@/features/sales-orders/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { mapWorkItemDto } from "@/features/work-items/types"
import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"

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
    return (
        <SalesOrderDetailContent
            key={salesOrderId}
            salesOrderId={salesOrderId}
            section={section}
        />
    )
}

function SalesOrderDetailContent({
    salesOrderId,
    section,
}: {
    salesOrderId: string
    section?: string
}) {
    const query = useSalesOrderDetailQuery(salesOrderId, true)
    const account = useAccountProfileQuery()
    const canViewExtraCost = hasPermission(
        account.data?.permissions,
        "cost_entry:list",
    )
    const canReadDelivery =
        !account.isError &&
        hasPermission(account.data?.permissions, "customer_acceptance:list")
    const deliveryQuery = useSalesOrderDeliveryProgressQuery(
        salesOrderId,
        Boolean(
            query.data?.nature === "physical_service" &&
            query.data.currentRevisionNo != null &&
            canReadDelivery,
        ),
        query.data?.lineItems,
    )
    const canOperateAcceptance =
        !account.isError &&
        Boolean(
            account.data?.userid &&
            account.data.userid === query.data?.ownerUserId,
        ) &&
        hasPermission(
            account.data?.permissions,
            "customer_acceptance:create",
        ) &&
        hasPermission(account.data?.permissions, "customer_acceptance:post")
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
        ) => {
            selectUrlSection(
                next,
                focusedWorkItem?.workItemType ===
                    "CUSTOMER_ACCEPTANCE_REGISTRATION",
                extras,
            )
        },
        [focusedWorkItem?.workItemType, selectUrlSection],
    )

    const [editingChange, setEditingChange] =
        React.useState<SalesChangeOrderSummary | null>(null)
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
            if (next.status === "succeeded") {
                setResult(null)
                toast.add({
                    title: next.title,
                    description: [
                        next.description,
                        next.nextResponsible
                            ? `下一步：${next.nextResponsible}`
                            : undefined,
                    ]
                        .filter(Boolean)
                        .join(" "),
                    type: "success",
                    timeout: 5000,
                })
                return
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
            <DetailWorkspaceSkeleton
                title="销售单"
                description="正在加载详情…"
                summary
                sidebar
            />
        )
    }

    if (query.isError && !query.data) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="销售单"
                    back={{
                        id: "sales-orders-detail-error-back",
                        label: backLabel,
                        href: backHref,
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="max-w-3xl py-6">
                        <BusinessFailureState
                            title="销售单加载失败"
                            error={query.error}
                            onRetry={() => {
                                void query.refetch()
                            }}
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!order) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="销售单不存在"
                    back={{
                        id: "sales-orders-detail-not-found-back",
                        label: "返回列表",
                        href: "/sales/orders",
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到这张销售单"
                            description="可能编号有误，或当前角色无权查看。"
                            action={
                                <Button
                                    id="sales-orders-detail-not-found-action"
                                    size="sm"
                                    render={<Link href="/sales/orders" />}
                                >
                                    返回销售单列表
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const canAddExpensePurchase =
        order.nature === "physical_service" &&
        order.currentRevisionNo != null &&
        hasPermission(account.data?.permissions, "purchase_order:create")

    const derived = deriveSalesOrderDetailState(order, {
        section,
        fromWorkspace,
        returnTo,
        hasEligibleAcceptance:
            canReadDelivery &&
            canOperateAcceptance &&
            !deliveryQuery.isError &&
            (deliveryQuery.data?.pendingFactCount ?? 0) > 0,
        canReadAcceptance: canReadDelivery,
        canViewExtraCost,
        canAddExpensePurchase,
        canSupplementContract: canSupplementSalesOrderContract(
            order,
            account.data?.permissions,
        ),
    })
    if (
        derived.showEditor &&
        hasPermission(account.data?.permissions, "sales_order:update")
    ) {
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

    const group = salesOrderSectionGroup(derived.navSection)

    return (
        <DetailWorkspacePage>
            <SalesOrderRelatedProblems
                order={order}
                onRetry={() => query.refetch({ throwOnError: true })}
            />
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
                primaryAction={
                    <SalesOrderStartChangeAction
                        order={order}
                        showChangeAction={
                            !order.activeChangeOrder ||
                            (derived.navSection !== "overview" &&
                                derived.navSection !== "versions")
                        }
                        canStartChange={derived.canStartChange}
                        changeBlocker={derived.changeBlocker}
                        changePending={startChangeCommand.isPending}
                        onOpenChangeConfirm={() => {
                            if (
                                order.activeChangeOrder?.statusCode === "DRAFT"
                            ) {
                                setEditingChange(order.activeChangeOrder)
                                return
                            }
                            void startChangeCommand
                                .startChange({
                                    order,
                                    commandLedger,
                                    onResult: handleActionResult,
                                })
                                .then((change) => {
                                    if (change) setEditingChange(change)
                                })
                                .catch(() => undefined)
                        }}
                    />
                }
                secondaryActions={
                    <SalesOrderDetailSecondaryActions
                        order={order}
                        onApprovalResult={handleActionResult}
                    />
                }
                tabs={
                    <SalesOrderDetailSectionTabs
                        order={order}
                        section={section}
                        navSection={derived.navSection}
                        visibleNav={derived.visibleNav}
                        canAccept={derived.canAccept}
                        onSelectSection={selectSection}
                    />
                }
            />

            <DetailWorkspaceBody
                summary={<SalesOrderAmountSummary order={order} />}
                alerts={
                    <>
                        {order.nature === "physical_service" &&
                        order.currentRevisionNo != null &&
                        order.primaryStatus.code !== "voided" ? (
                            <SalesOrderProcurementAlert
                                progress={order.related.procurementProgress}
                            />
                        ) : null}

                        {result ? (
                            <FormalActionResult
                                status={result.status}
                                title={result.title}
                                description={result.description}
                                actions={
                                    result.status !== "unknown" ? (
                                        <Button
                                            id="sales-order-result-dismiss"
                                            variant="ghost"
                                            size="sm"
                                            onClick={() => setResult(null)}
                                        >
                                            关闭提示
                                        </Button>
                                    ) : undefined
                                }
                                reference={result.reference}
                                facts={
                                    result.nextResponsible
                                        ? [
                                              {
                                                  label: "下一步",
                                                  value: result.nextResponsible,
                                              },
                                          ]
                                        : undefined
                                }
                            />
                        ) : null}

                        {derived.navSection === "overview" &&
                        order.activeChangeOrder ? (
                            <SalesChangeOrderApprovalSection
                                key={order.activeChangeOrder.id}
                                readonlyApproval
                                salesOrderId={order.id}
                                nature={order.nature}
                                changeOrder={order.activeChangeOrder}
                                onResult={handleActionResult}
                            />
                        ) : null}

                        {section === "change-review" &&
                        changeOrderId &&
                        changeQuery.isPending ? (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
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
                                expectedTaskVersion={
                                    focusedWorkItem?.taskVersion
                                }
                                workItemAllowedActions={
                                    focusedWorkItem?.allowedActions
                                }
                                onResult={handleActionResult}
                            />
                        ) : null}
                    </>
                }
                sidebar={
                    group === "timeline" ? undefined : (
                        <SalesOrderDetailSidebar
                            order={order}
                            onSelectSection={selectSection}
                        />
                    )
                }
                sidebarLabel={
                    group === "timeline" ? undefined : "履约与票款进度"
                }
            >
                <SalesOrderDetailSectionContent
                    order={order}
                    section={section}
                    navSection={derived.navSection}
                    visibleNav={derived.visibleNav}
                    canAccept={derived.canAccept}
                    canOperateAcceptance={canOperateAcceptance}
                    canReadDelivery={canReadDelivery}
                    canViewExtraCost={canViewExtraCost}
                    canAddExpensePurchase={canAddExpensePurchase}
                    deliveryQuery={deliveryQuery}
                    onSelectSection={selectSection}
                    onApprovalResult={handleActionResult}
                />
            </DetailWorkspaceBody>

            {editingChange ? (
                <SalesChangeOrderEditDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setEditingChange(null)
                    }}
                    salesOrderId={order.id}
                    changeOrderId={editingChange.id}
                    nature={order.nature}
                    approval={editingChange.approval}
                    onApplied={() => {
                        handleActionResult({
                            status: "succeeded",
                            title: "改单已提交审批",
                            description:
                                "审批通过后生成新版本，进度见「变更与售后」。",
                        })
                        selectSection("versions")
                    }}
                />
            ) : null}
        </DetailWorkspacePage>
    )
}
