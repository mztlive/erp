"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
import { welfareScenarioLabel } from "@/lib/business-options"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { CardSalesApprovalPanel } from "@/features/sales-orders/components/card-sales-approval-panel"
import { SalesOrderApprovalArea } from "@/features/sales-orders/components/sales-order-approval-area"
import { salesOrderApprovalPhase } from "@/features/sales-orders/lib/sales-order-approval"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { cn } from "@/lib/utils"

function OverviewField({
    label,
    value,
    numeric,
}: {
    label: string
    value: React.ReactNode
    numeric?: boolean
}) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className={cn("mt-0.5 truncate text-sm", numeric && "num")}>
                {value}
            </dd>
        </div>
    )
}

export function LineItemsTable({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="overflow-x-auto">
            <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left">
                    <tr>
                        <th className="px-3 py-1.5 font-medium">项目</th>
                        <th className="px-3 py-1.5 font-medium">数量</th>
                        {isCard ? (
                            <th className="px-3 py-1.5 font-medium">
                                面额 / 形态
                            </th>
                        ) : (
                            <th className="px-3 py-1.5 font-medium">
                                交付方式
                            </th>
                        )}
                        <th className="px-3 py-1.5 font-medium text-right">
                            含税金额
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {order.lineItems.map((line) => (
                        <tr key={line.id} className="border-t border-border/30">
                            <td className="px-3 py-1.5">
                                <div>{line.name}</div>
                                {line.sku ? (
                                    <div className="num text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </td>
                            <td className="num px-3 py-1.5">
                                {line.quantity} {line.unit}
                            </td>
                            {isCard ? (
                                <td className="px-3 py-1.5 text-sm">
                                    {line.faceValue ? (
                                        <MoneyValue value={line.faceValue} />
                                    ) : (
                                        "—"
                                    )}
                                    {line.cardForm ? (
                                        <span className="mt-0.5 block text-xs text-muted-foreground">
                                            {line.cardForm}
                                        </span>
                                    ) : null}
                                </td>
                            ) : (
                                <td className="px-3 py-1.5 text-sm text-muted-foreground">
                                    <div>{line.fulfillmentMode ?? "—"}</div>
                                    {line.dueDate ? (
                                        <div className="num mt-0.5 text-xs">
                                            {line.dueDate}
                                        </div>
                                    ) : null}
                                </td>
                            )}
                            <td className="px-3 py-1.5 text-right">
                                <MoneyValue
                                    value={line.amountGross}
                                    taxBasis="gross"
                                />
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    )
}

export function OverviewPanel({
    order,
    showApproval,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    showApproval: boolean
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-4">
            {order.nature === "physical_service" && order.approval ? (
                <SalesOrderApprovalArea
                    phase={salesOrderApprovalPhase(
                        order.approval,
                        order.primaryStatus.code,
                    )}
                    approval={order.approval}
                    documentId={order.id}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={(view: ApprovalCommandView) =>
                        onApprovalResult?.({
                            status: "succeeded",
                            title: "审批决定已提交",
                            description: view.latestRejectionReason
                                ? `已按当前任务提交决定。${view.latestRejectionReason}`
                                : "已按当前任务提交决定。",
                            reference: order.documentNumber,
                            nextResponsible: view.currentAssigneeName,
                        })
                    }
                />
            ) : null}

            {showApproval && order.activeCardSalesApproval ? (
                <CardSalesApprovalPanel
                    order={order}
                    approval={order.activeCardSalesApproval}
                    onResult={onApprovalResult}
                />
            ) : null}

            {showApproval && order.cardApprovalProjectionBlocker ? (
                <Alert variant="destructive">
                    <AlertTitle>审批信息未就绪</AlertTitle>
                    <AlertDescription>
                        {order.cardApprovalProjectionBlocker}
                    </AlertDescription>
                </Alert>
            ) : null}

            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 xl:grid-cols-3">
                <OverviewField
                    label="关联合同"
                    value={order.contractRevisionLabel || "—"}
                />
                <OverviewField
                    label="福利场景"
                    value={welfareScenarioLabel(order.welfareScene)}
                />
                <OverviewField
                    label="付款条件"
                    value={order.paymentTerms || "—"}
                />
                <OverviewField
                    label={isCard ? "履约期限（到期交付）" : "履约期限"}
                    value={order.fulfillmentDeadline || "—"}
                    numeric
                />
                <OverviewField
                    label="客户联系人"
                    value={order.customerContact ?? "—"}
                />
                <OverviewField
                    label="当前版本"
                    value={`v${order.version}`}
                    numeric
                />
            </dl>

            <div>
                <div className="mb-2 flex items-baseline justify-between gap-2">
                    <h2 className="text-sm font-medium">
                        {isCard ? "卡券明细" : "销售明细"}
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        {order.lineItems.length} 行
                    </p>
                </div>
                <LineItemsTable order={order} />
            </div>
        </div>
    )
}
