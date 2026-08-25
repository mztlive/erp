"use client"

import * as React from "react"
import Link from "next/link"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import { FulfillmentOperationsWorkspace } from "@/features/fulfillment-operations/pages/components/fulfillment-operations-workspace"
import { FulfillmentPageStates } from "@/features/fulfillment-operations/pages/components/fulfillment-page-states"
import { useFulfillmentKeyboardShortcuts } from "@/features/fulfillment-operations/pages/hooks/use-fulfillment-keyboard-shortcuts"
import { useFulfillmentOperationsController } from "@/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import { fulfillmentWorkspaceHref } from "@/features/sales-orders/lib/sales-order-detail-model"
import { cn } from "@/lib/utils"

export function FulfillmentPanel({
    order,
    selfReturn,
    onOpenAcceptance,
    onDataChanged,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    onOpenAcceptance: () => void
    onDataChanged: () => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="flex flex-col gap-4">
            <SectionLead>
                {isCard
                    ? "卡券到期即算交付完成。消费多少不影响本单是否交付完成。"
                    : "只处理本销售单待入库、发货、电子交付和服务记录。岗位队列请到履约工作台。"}
            </SectionLead>

            <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                {isCard ? (
                    <>
                        <h3 className="text-sm font-medium">卡券交付</h3>
                        <p className="mt-1 text-xs text-muted-foreground">
                            期限 {order.fulfillmentDeadline || "—"} · 当前{" "}
                            {order.fulfillment.label}
                        </p>
                    </>
                ) : (
                    <>
                        <div className="flex flex-wrap items-start justify-between gap-3">
                            <div className="min-w-0">
                                <h3 className="text-sm font-medium">
                                    本单履约
                                </h3>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    采购单 {order.related.purchaseOrders} 笔 ·
                                    履约 {order.fulfillment.label} · 履约单据{" "}
                                    {order.related.fulfillments} 笔
                                </p>
                            </div>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link
                                        href={fulfillmentWorkspaceHref(
                                            order,
                                            selfReturn,
                                        )}
                                    />
                                }
                            >
                                打开履约工作台
                            </Button>
                        </div>
                    </>
                )}
            </div>

            {isCard ? null : (
                <SalesOrderFulfillmentWork
                    salesOrderId={order.id}
                    onOpenAcceptance={onOpenAcceptance}
                    onDataChanged={onDataChanged}
                />
            )}
        </div>
    )
}

function SalesOrderFulfillmentWork({
    salesOrderId,
    onOpenAcceptance,
    onDataChanged,
}: {
    salesOrderId: string
    onOpenAcceptance: () => void
    onDataChanged: () => void
}) {
    const profileQuery = useAccountProfileQuery()
    const filters = React.useMemo(
        (): FulfillmentQueueFilters => ({
            role: "sales_order",
            salesOrderId,
        }),
        [salesOrderId],
    )
    const controller = useFulfillmentOperationsController({
        roleValue: "sales_order",
        filters,
        lane: null,
        autoNextExplicit: "0",
        stateMode: "local",
        grantedPermissions: profileQuery.data?.permissions ?? [],
        permissionsReady: !profileQuery.isPending,
        onPosted: () => onDataChanged(),
    })

    useFulfillmentKeyboardShortcuts({
        dirty: controller.dirty,
        canPost: controller.canPost,
        formalPending: controller.formalPending,
        canExecute: controller.canExecute,
        supportsSave: controller.supportsSave,
        onSave: () => void controller.handleSave(),
        onConfirm: () => controller.setConfirmOpen(true),
        onNavigate: controller.handleNavigate,
        onToggleShortcuts: () => controller.setShortcutsOpen((value) => !value),
    })

    if (controller.queueQuery.isPending || controller.queueQuery.isError) {
        return (
            <FulfillmentPageStates
                status={controller.queueQuery.isPending ? "pending" : "error"}
                standalone
                embedded
                headerDescription="本单履约"
                error={controller.queueQuery.error}
                onRetry={() => void controller.queueQuery.refetch()}
            />
        )
    }

    return (
        <FulfillmentOperationsWorkspace
            controller={controller}
            headerDescription="本单履约"
            roleLabel="本单履约"
            embedded
            onBack={() => undefined}
            onOpenAcceptance={onOpenAcceptance}
        />
    )
}
