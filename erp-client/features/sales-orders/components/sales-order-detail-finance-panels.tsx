"use client"

import * as React from "react"

import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import { RevisionHistoryCard } from "@/features/sales-orders/components/revision-history-card"
import { SalesOrderCollaborationCard } from "@/features/execution-projections/collaboration-card"
import { CustomerReceivablesWorkspace } from "@/features/customer-receivables/components/customer-receivables-workspace"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"

export function ReceivablePanel({
    order,
    onDataChanged,
}: {
    order: SalesOrderDetailView
    onDataChanged: () => void
}) {
    return (
        <CustomerReceivablesWorkspace
            embedded
            salesOrderId={order.id}
            salesOrderNo={order.documentNumber}
            counterpartyPartyId={order.settlementPartyId}
            counterpartyPartyName={order.settlementEntity}
            customerId={order.customerId}
            customerName={order.customerName}
            onSalesOrderChanged={onDataChanged}
        />
    )
}

export function VersionsPanel({
    order,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    return (
        <div className="space-y-4">
            <RevisionHistoryCard
                revisions={order.revisions}
                currentVersion={order.currentRevisionNo}
                contractRevisionLabel={order.contractRevisionLabel}
            />
            {order.activeChangeOrder ? (
                <SalesChangeOrderApprovalSection
                    salesOrderId={order.id}
                    nature={order.nature}
                    changeOrder={order.activeChangeOrder}
                    onResult={onApprovalResult}
                />
            ) : null}
        </div>
    )
}

export function CollaborationPanel({ order }: { order: SalesOrderDetailView }) {
    if (order.nature !== "card_voucher") {
        return (
            <p className="text-sm text-muted-foreground">
                只有卡券销售单会与商城对接。
            </p>
        )
    }
    return (
        <div className="space-y-4">
            <SectionLead>
                这里只看商城接收和执行投影，不提供第二套改单入口。
            </SectionLead>
            <SalesOrderCollaborationCard
                salesOrderId={order.id}
                salesOrderNo={order.documentNumber}
            />
        </div>
    )
}
