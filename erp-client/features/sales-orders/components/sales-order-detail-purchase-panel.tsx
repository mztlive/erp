"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import {
    ExpensePurchaseCreateDialog,
    type ExpensePurchasePrefill,
} from "@/features/purchase-orders/components/expense-purchase-create-dialog"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import {
    SalesOrderDeliveryOverview,
    type DeliveryProgressQuery,
} from "./sales-order-delivery-overview"

/** 销售单交付分区：展示商品交付情况，并支持追加附加费用。 */
export function PurchasePanel({
    order,
    deliveryQuery,
    canReadDelivery,
}: {
    order: SalesOrderDetailView
    deliveryQuery: DeliveryProgressQuery
    canReadDelivery: boolean
}) {
    const permissions = useSalesOrderDetailPermissions()
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const [expensePrefill, setExpensePrefill] = React.useState<
        ExpensePurchasePrefill | undefined
    >(undefined)
    const freightFollowUps = order.related.freightFollowUps ?? []
    const [expenseCreated, setExpenseCreated] = React.useState<string | null>(
        null,
    )
    const canAddExpense =
        order.nature === "physical_service" &&
        order.currentRevisionNo != null &&
        hasPermission(permissions.granted, "purchase_order:create")
    return (
        <div className="space-y-6">
            <SalesOrderDeliveryOverview
                order={order}
                query={deliveryQuery}
                canRead={canReadDelivery && order.currentRevisionNo != null}
            />
            {canAddExpense && freightFollowUps.length ? (
                <section
                    aria-labelledby="sales-orders-detail-freight-title"
                    className="space-y-2 rounded-lg border border-warning-border bg-warning-soft px-4 py-3"
                >
                    <h3
                        id="sales-orders-detail-freight-title"
                        className="text-sm font-medium text-warning-soft-foreground"
                    >
                        运费待追加
                    </h3>
                    <p className="text-xs text-warning-soft-foreground">
                        以下商品为集采运费另计。供应商收取运费时追加物流费用采购；供应商不收运费时在采购单上标记。
                    </p>
                    <ul className="divide-y divide-warning-border">
                        {freightFollowUps.map((item) => (
                            <li
                                key={`${item.purchaseOrderId}-${item.salesOrderLineId}`}
                                className="flex flex-wrap items-center justify-between gap-2 py-2 text-sm"
                            >
                                <span className="min-w-0">
                                    {item.productName ?? "商品"}
                                    <span className="ml-2 text-xs text-muted-foreground">
                                        {item.supplierName ?? "供应商"} · 采购单{" "}
                                        {item.purchaseNo || "草稿"}
                                    </span>
                                </span>
                                <Button
                                    id={`sales-orders-detail-freight-add-${toAutomationIdSegment(item.purchaseOrderId)}-${toAutomationIdSegment(item.salesOrderLineId)}`}
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() => {
                                        setExpensePrefill({
                                            supplierId: item.supplierId,
                                            salesOrderLineId:
                                                item.salesOrderLineId,
                                            description: `${item.productName ?? "商品"} 运费`,
                                        })
                                        setExpenseOpen(true)
                                    }}
                                >
                                    追加运费
                                </Button>
                            </li>
                        ))}
                    </ul>
                </section>
            ) : null}

            {canAddExpense ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-dashed px-4 py-3">
                    <p className="text-sm text-muted-foreground">
                        {expenseCreated
                            ? `已创建费用采购单 ${expenseCreated}，已提交审批。`
                            : "本单有我方承担、不对应销售明细的费用（第三方物流、额外包装、上门安装等）时，追加费用采购单走审批和付款。"}
                    </p>
                    <Button
                        id="sales-orders-detail-add-expense-purchase"
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                            setExpensePrefill(undefined)
                            setExpenseOpen(true)
                        }}
                    >
                        追加附加费用
                    </Button>
                </div>
            ) : null}

            <ExpensePurchaseCreateDialog
                salesOrderId={order.id}
                salesLines={order.lineItems
                    .filter((line) => line.quantity !== "0")
                    .map((line) => ({ id: line.id, name: line.name }))}
                open={expenseOpen}
                onOpenChange={setExpenseOpen}
                prefill={expensePrefill}
                onCreated={(created) => setExpenseCreated(created.purchaseNo)}
            />
        </div>
    )
}
