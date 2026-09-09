"use client"

import type * as React from "react"
import { useRouter } from "next/navigation"
import {
    ArrowLeftIcon,
    FilePenLineIcon,
    SendIcon,
    Trash2Icon,
} from "lucide-react"

import {
    DocumentHeader,
    FormalActionResult,
    PageActions,
    PageHeader,
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
            <PageHeader
                variant="object-chrome"
                metadata={
                    <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span
                            ref={titleRef}
                            tabIndex={-1}
                            id="procurement-orders-detail-heading"
                            className="text-sm font-medium text-muted-foreground outline-none"
                        >
                            采购单
                        </span>
                        {mode === "edit" ? <span>{modeLabel}</span> : null}
                    </span>
                }
                actions={
                    <PageActions
                        id="procurement-orders-detail-navigation"
                        actions={[
                            {
                                actionKey: "back",
                                label: "返回列表",
                                icon: ArrowLeftIcon,
                                variant: "outline",
                                onClick: () =>
                                    requestLeave(() =>
                                        router.push("/procurement/orders"),
                                    ),
                                id: "procurement-orders-detail-back",
                            },
                        ]}
                    />
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

            <DocumentHeader
                density="compact"
                className="border-0 pb-0 [&>div:first-child]:flex-col sm:[&>div:first-child]:flex-row [&_h1]:wrap-anywhere [&_h1]:text-3xl"
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
                secondaryActions={
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
        </>
    )
}
