"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { ExpensePurchaseStartDialog } from "./expense-purchase-start-dialog"

import * as React from "react"
import { PurchaseOrderCorrespondence } from "./purchase-order-correspondence"
import { useRouter, useSearchParams } from "next/navigation"
import { FilePenLineIcon, SendIcon, Trash2Icon } from "lucide-react"

import {
    DetailPageHeader,
    FormalActionResult,
    PageActions,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

import { PurchaseOrderCancelApprovalButton } from "@/features/purchase-orders/components/purchase-order-cancel-approval-button"
import {
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import type { PurchaseOrderDetailMode } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"

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
    result,
    onDismissResult,
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
    result: PurchaseOrderDetailResult | null
    onDismissResult: () => void
}) {
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const searchParams = useSearchParams()
    const returnTo = searchParams.get("returnTo")
    const returnPath = returnTo?.split("?")[0]
    const workspaceReturn =
        searchParams.get("from") === "workspace" &&
        (returnPath === "/workspace" || returnPath === "/workspace/tasks")
            ? returnTo
            : null
    const profile = useAccountProfileQuery()
    const canCreateExpense =
        !profile.isError &&
        hasPermission(profile.data?.permissions, "purchase_order:create") &&
        mode !== "edit" &&
        order.identity.status !== "VOID"
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
            <DetailPageHeader
                back={{
                    id: "procurement-orders-detail-back",
                    label: workspaceReturn ? "返回工作台" : "采购单列表",
                    onClick: () =>
                        requestLeave(() =>
                            router.push(
                                workspaceReturn ?? "/procurement/orders",
                            ),
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
                primaryStatus={{
                    label: order.identity.statusLabel,
                    tone: order.identity.statusTone,
                }}
                titleExtra={
                    <Badge variant="secondary" className="font-normal">
                        {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
                    </Badge>
                }
                primaryAction={
                    <div className="flex flex-wrap items-center gap-2">
                        {mode !== "edit" ? (
                            <PurchaseOrderCorrespondence order={order} />
                        ) : null}
                        <PurchaseOrderCancelApprovalButton
                            order={order}
                            onResult={onCancelResult}
                        />
                        <PageActions
                            id="procurement-orders-detail-actions"
                            actions={[
                                ...(canCreateExpense
                                    ? [
                                          {
                                              actionKey: "add-expense",
                                              label: "追加费用",
                                              variant: "outline" as const,
                                              id: "procurement-orders-detail-add-expense",
                                              onClick: () =>
                                                  setExpenseOpen(true),
                                          },
                                      ]
                                    : []),
                                ...(canEdit && mode !== "edit"
                                    ? [
                                          {
                                              actionKey: "edit",
                                              label: "编辑草稿",
                                              icon: FilePenLineIcon,
                                              variant: "outline" as const,
                                              onClick: () =>
                                                  router.push(
                                                      `${baseHref}?mode=edit`,
                                                  ),
                                              id: "procurement-orders-detail-edit",
                                          },
                                      ]
                                    : []),
                                ...(canChange || continueChange
                                    ? [
                                          {
                                              actionKey: "change",
                                              label: changePending
                                                  ? "正在打开改单…"
                                                  : continueChange
                                                    ? "继续改单"
                                                    : "发起改单",
                                              icon: FilePenLineIcon,
                                              variant: "outline" as const,
                                              disabled: changePending,
                                              onClick: () => onRequestChange(),
                                              id: "procurement-orders-detail-change",
                                          },
                                      ]
                                    : []),
                                ...(canVoid && mode !== "edit"
                                    ? [
                                          {
                                              actionKey: "void",
                                              label: "作废草稿",
                                              icon: Trash2Icon,
                                              variant: "outline" as const,
                                              onClick: () => onRequestVoid(),
                                              id: "procurement-orders-detail-void",
                                          },
                                      ]
                                    : []),
                                ...(canSubmit
                                    ? [
                                          {
                                              actionKey: "submit",
                                              label: "提交审批",
                                              icon: SendIcon,
                                              onClick: () => onRequestSubmit(),
                                              id: "procurement-orders-detail-submit",
                                          },
                                      ]
                                    : []),
                            ]}
                        />
                    </div>
                }
            />
            {result ? (
                <FormalActionResult
                    status={result.status}
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={
                        <Button
                            id="procurement-orders-detail-result-close"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onDismissResult}
                        >
                            关闭
                        </Button>
                    }
                />
            ) : null}
        </>
    )
}
