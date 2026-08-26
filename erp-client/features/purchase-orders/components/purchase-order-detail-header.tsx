"use client"

import type * as React from "react"
import { useRouter } from "next/navigation"
import {
    ArrowLeftIcon,
    FilePenLineIcon,
    SendIcon,
    ShieldCheckIcon,
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
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import type { PurchaseOrderDetailMode } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { fulfillmentTasksHref } from "@/features/workspace/lib/fulfillment-destination"

export function PurchaseOrderDetailHeader({
    order,
    mode,
    displayNo,
    modeLabel,
    titleRef,
    router,
    baseHref,
    w12PayHref,
    w27SettleHref,
    canPay,
    canFulfill,
    canEdit,
    canSubmit,
    canVoid,
    canOpenReview,
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
    w12PayHref: string
    w27SettleHref: string
    canPay: boolean
    canFulfill: boolean
    canEdit: boolean
    canSubmit: boolean
    canVoid: boolean
    canOpenReview: boolean
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
                            className="text-xl font-semibold tracking-tight text-foreground outline-none"
                        >
                            采购单
                        </span>
                        <span>{modeLabel}</span>
                    </span>
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <PageActions
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
                                },
                                ...(canPay
                                    ? [
                                          {
                                              actionKey: "pay",
                                              label: "去供应商往来",
                                              variant: "outline" as const,
                                              onClick: () =>
                                                  router.push(w12PayHref),
                                          },
                                          {
                                              actionKey: "settle",
                                              label: "去对账结算",
                                              variant: "outline" as const,
                                              onClick: () =>
                                                  router.push(w27SettleHref),
                                          },
                                      ]
                                    : []),
                                ...(canFulfill
                                    ? [
                                          {
                                              actionKey: "fulfill",
                                              label: "去交付",
                                              variant: "outline" as const,
                                              onClick: () =>
                                                  router.push(
                                                      fulfillmentTasksHref(
                                                          order.identity
                                                              .purchaseOrderId,
                                                      ),
                                                  ),
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
                                          },
                                      ]
                                    : []),
                                ...(canOpenReview && mode !== "review"
                                    ? [
                                          {
                                              actionKey: "review",
                                              label: "打开审核",
                                              icon: ShieldCheckIcon,
                                              onClick: () =>
                                                  router.push(
                                                      `${baseHref}?mode=review`,
                                                  ),
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
                title={order.header.supplierSnapshot || "采购单"}
                documentNumber={displayNo}
                primaryStatus={{
                    label: order.identity.statusLabel,
                    tone: order.identity.statusTone,
                }}
                version={
                    order.identity.revisionNo == null
                        ? "尚未生效"
                        : `v${order.identity.revisionNo}`
                }
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                        <span>来源 {order.header.salesOrderNo}</span>
                        <span aria-hidden="true">·</span>
                        <Badge variant="secondary" className="font-normal">
                            {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
                        </Badge>
                        <Badge variant="secondary" className="font-normal">
                            {
                                FULFILLMENT_RESPONSIBILITY_LABEL[
                                    order.header.fulfillmentResponsibility
                                ]
                            }
                        </Badge>
                    </span>
                }
                statuses={[
                    {
                        id: "review",
                        label: "审批",
                        status: {
                            label: order.identity.reviewLabel,
                            tone:
                                order.identity.reviewStatus === "PENDING"
                                    ? "warning"
                                    : order.identity.reviewStatus === "APPROVED"
                                      ? "success"
                                      : order.identity.reviewStatus ===
                                          "REJECTED"
                                        ? "destructive"
                                        : "neutral",
                        },
                    },
                    {
                        id: "payment",
                        label: "付款",
                        status: {
                            label: order.progress.payment,
                            tone:
                                order.progress.payment === "已付"
                                    ? "success"
                                    : order.progress.payment === "部分"
                                      ? "info"
                                      : "neutral",
                        },
                    },
                    {
                        id: "invoice",
                        label: "进项票",
                        status: {
                            label: order.progress.invoice,
                            tone:
                                order.progress.invoice === "完成"
                                    ? "success"
                                    : order.progress.invoice === "部分"
                                      ? "info"
                                      : "neutral",
                        },
                    },
                    {
                        id: "fulfillment",
                        label: "履约",
                        status: {
                            label: order.progress.fulfillment,
                            tone: order.fulfillmentSummary.progressTone,
                        },
                    },
                ]}
            />
        </>
    )
}
