"use client"

import type * as React from "react"
import { useRouter } from "next/navigation"
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
    requestLeave: (go: () => void) => void
    onRequestVoid: () => void
    onRequestChange: () => void
    onRequestSubmit: () => void
    onCancelResult: (result: PurchaseOrderDetailResult) => void
    result: PurchaseOrderDetailResult | null
    onDismissResult: () => void
}) {
    return (
        <>
            <DetailPageHeader
                back={{
                    id: "procurement-orders-detail-back",
                    label: "采购单列表",
                    onClick: () =>
                        requestLeave(() => router.push("/procurement/orders")),
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
                        <PageActions
                            id="procurement-orders-detail-actions"
                            actions={[
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
                                ...(canVoid && mode !== "edit"
                                    ? [
                                          {
                                              actionKey: "void",
                                              label: "作废草稿",
                                              icon: Trash2Icon,
                                              variant: "destructive" as const,
                                              onClick: () => onRequestVoid(),
                                              id: "procurement-orders-detail-void",
                                          },
                                      ]
                                    : []),
                                ...(canChange
                                    ? [
                                          {
                                              actionKey: "change",
                                              label: "发起采购变更",
                                              variant: "outline" as const,
                                              onClick: () => onRequestChange(),
                                              id: "procurement-orders-detail-change",
                                          },
                                      ]
                                    : []),
                            ]}
                        />
                        <PurchaseOrderCancelApprovalButton
                            order={order}
                            onResult={onCancelResult}
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
