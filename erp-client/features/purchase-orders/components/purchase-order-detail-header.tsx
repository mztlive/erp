"use client"

import * as React from "react"
import { FilePenLineIcon, PlusIcon, SendIcon, Trash2Icon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { PurchaseOrderCancelApprovalButton } from "@/features/purchase-orders/components/purchase-order-cancel-approval-button"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import type { PurchaseOrderDetailMode } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import {
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { ExpensePurchaseStartDialog } from "./expense-purchase-start-dialog"
import { PurchaseOrderCorrespondence } from "./purchase-order-correspondence"

export function PurchaseOrderDetailHeader({
    order,
    mode,
    displayNo,
    modeLabel,
    titleRef,
    router,
    baseHref,
    canEdit,
    canSubmit,
    canVoid,
    canChange,
    continueChange,
    changePending,
    requestLeave,
    onRequestVoid,
    onRequestChange,
    onRequestSubmit,
    onCancelResult,
    tabs,
}: {
    order: PurchaseOrderCenterView
    mode: PurchaseOrderDetailMode
    displayNo: string
    modeLabel: string
    titleRef: React.RefObject<HTMLHeadingElement | null>
    router: ReturnType<typeof useRouter>
    baseHref: string
    canEdit: boolean
    canSubmit: boolean
    canVoid: boolean
    canChange: boolean
    /** 已有可编辑的改单草稿，入口显示「继续改单」。 */
    continueChange: boolean
    changePending: boolean
    requestLeave: (go: () => void) => void
    onRequestVoid: () => void
    onRequestChange: () => void
    onRequestSubmit: () => void
    onCancelResult: (result: PurchaseOrderDetailResult) => void
    tabs?: React.ReactNode
}) {
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const searchParams = useSearchParams()
    const returnTo = searchParams.get("returnTo")
    const internalReturn =
        returnTo?.startsWith("/") && !returnTo.startsWith("//")
            ? returnTo
            : null
    const returnPath = internalReturn?.split("?")[0]
    const workspaceReturn =
        searchParams.get("from") === "workspace" &&
        (returnPath === "/workspace" ||
            returnPath === "/workspace/tasks" ||
            /^\/workspace\/tasks\/[^/]+$/.test(returnPath ?? ""))
            ? internalReturn
            : null
    const paymentRequestReturn = internalReturn?.startsWith(
        "/procurement/payment-requests/",
    )
        ? internalReturn
        : null
    const backTarget = workspaceReturn ?? paymentRequestReturn
    const profile = useAccountProfileQuery()
    const canCreateExpense =
        !profile.isError &&
        hasPermission(profile.data?.permissions, "purchase_order:create") &&
        mode !== "edit" &&
        order.identity.status !== "VOID"

    const changeIsPrimary = continueChange && !canSubmit

    return (
        <>
            <ExpensePurchaseStartDialog
                open={expenseOpen}
                onOpenChange={setExpenseOpen}
                salesOrderId={order.header.salesOrderId}
                supplierId={order.header.supplierId}
                onCreated={(created) =>
                    router.push(
                        `/procurement/orders/${created.purchaseOrderId}`,
                    )
                }
            />
            <DetailWorkspaceChrome
                back={{
                    id: "procurement-orders-detail-back",
                    label: workspaceReturn
                        ? "返回工作台"
                        : paymentRequestReturn
                          ? "返回付款申请"
                          : "采购单列表",
                    onClick: () =>
                        requestLeave(() =>
                            router.push(backTarget ?? "/procurement/orders"),
                        ),
                }}
                navigationMeta={mode === "edit" ? modeLabel : undefined}
                headingProps={{
                    id: "procurement-orders-detail-heading",
                    ref: titleRef,
                    tabIndex: -1,
                    className: "outline-none",
                }}
                title={order.header.supplierSnapshot || "采购单"}
                documentNumber={displayNo}
                version={
                    order.identity.revisionNo != null
                        ? `v${order.identity.revisionNo}`
                        : undefined
                }
                primaryStatus={{
                    label: order.identity.statusLabel,
                    tone: order.identity.statusTone,
                }}
                titleExtra={
                    <>
                        <Badge variant="secondary" className="font-normal">
                            {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
                        </Badge>
                        {order.activeChangeOrder ? (
                            <DetailWorkspaceDirtyBadge label="改单中" />
                        ) : null}
                    </>
                }
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            order.identity.revisionNo == null
                                ? "尚未生效"
                                : null,
                            order.header.ownerName ? (
                                <span key="owner">
                                    采购负责人{" "}
                                    <span className="font-medium text-foreground">
                                        {order.header.ownerName}
                                    </span>
                                </span>
                            ) : null,
                            order.header.salesOrderNo ? (
                                <span key="sales" className="num">
                                    来源销售 {order.header.salesOrderNo}
                                </span>
                            ) : null,
                        ]}
                    />
                }
                secondaryActions={
                    <>
                        {mode !== "edit" ? (
                            <PurchaseOrderCorrespondence order={order} />
                        ) : null}
                        <PurchaseOrderCancelApprovalButton
                            order={order}
                            onResult={onCancelResult}
                            triggerVariant="ghost"
                            triggerClassName={
                                detailWorkspaceStyles.quietDangerButton
                            }
                        />
                        {canVoid && mode !== "edit" ? (
                            <Button
                                id="procurement-orders-detail-void"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={
                                    detailWorkspaceStyles.quietDangerButton
                                }
                                onClick={onRequestVoid}
                            >
                                <Trash2Icon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                作废草稿
                            </Button>
                        ) : null}
                        {canCreateExpense ? (
                            <Button
                                id="procurement-orders-detail-add-expense"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={() => setExpenseOpen(true)}
                            >
                                <PlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                追加费用
                            </Button>
                        ) : null}
                        {canEdit && mode !== "edit" ? (
                            <Button
                                id="procurement-orders-detail-edit"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={() =>
                                    router.push(`${baseHref}?mode=edit`)
                                }
                            >
                                <FilePenLineIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                编辑草稿
                            </Button>
                        ) : null}
                        {(canChange || continueChange) && !changeIsPrimary ? (
                            <Button
                                id="procurement-orders-detail-change"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={changePending}
                                onClick={onRequestChange}
                            >
                                <FilePenLineIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                {changePending
                                    ? "正在打开改单…"
                                    : continueChange
                                      ? "继续改单"
                                      : "发起改单"}
                            </Button>
                        ) : null}
                    </>
                }
                primaryAction={
                    canSubmit ? (
                        <Button
                            id="procurement-orders-detail-submit"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            onClick={onRequestSubmit}
                        >
                            <SendIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            提交审批
                        </Button>
                    ) : changeIsPrimary ? (
                        <Button
                            id="procurement-orders-detail-change"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            disabled={changePending}
                            onClick={onRequestChange}
                        >
                            <FilePenLineIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {changePending ? "正在打开改单…" : "继续改单"}
                        </Button>
                    ) : null
                }
                tabs={tabs}
            />
        </>
    )
}

/** 置于吸顶头下方的通栏无框金额摘要带：首屏即览，随页面自然滚走不挤占正文视野。 */
export function PurchaseOrderAmountSummary({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const amount = (value?: string) =>
        costMasked ? (
            <span className="num text-muted-foreground" aria-label="金额已隐藏">
                •••
            </span>
        ) : value == null ? (
            <span className="text-muted-foreground">—</span>
        ) : (
            <MoneyValue value={value} />
        )

    return (
        <DetailWorkspaceMetrics
            columns={4}
            variant="bar"
            ariaLabel="采购单金额摘要"
        >
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label="采购金额（含税）"
                value={amount(order.currentContent.totals.gross)}
                detail={
                    costMasked ? (
                        "含税/税额已按权限隐藏"
                    ) : (
                        <>
                            <span>
                                不含税{" "}
                                <MoneyValue
                                    value={order.currentContent.totals.net}
                                />
                            </span>
                            <span
                                aria-hidden="true"
                                className="select-none text-muted-foreground/40"
                            >
                                ·
                            </span>
                            <span>
                                税额{" "}
                                <MoneyValue
                                    value={order.currentContent.totals.tax}
                                />
                            </span>
                        </>
                    )
                }
            />
            <DetailWorkspaceMetricItem
                label="已付并核销"
                value={amount(order.payableSummary?.paidAllocatedAmount)}
            />
            <DetailWorkspaceMetricItem
                label="待付余额"
                value={amount(order.payableSummary?.payableOpenAmount)}
                detail={
                    !order.payableSummary ? "审批通过后形成应付" : undefined
                }
            />
            <DetailWorkspaceMetricItem
                label="已收票并核销"
                value={amount(
                    order.payableSummary?.purchaseInvoiceAllocatedAmount,
                )}
            />
        </DetailWorkspaceMetrics>
    )
}
