"use client"

import * as React from "react"
import Link from "next/link"
import { WalletIcon } from "lucide-react"

import { DocumentSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import { RevisionHistoryCard } from "@/features/sales-orders/components/revision-history-card"
import { SalesOrderCollaborationCard } from "@/features/execution-projections/collaboration-card"
import { RelatedLanes } from "@/features/sales-orders/components/sales-order-detail-related-lanes"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    receivableWorkspaceHref,
    type SalesOrderDetailActionResult,
} from "@/features/sales-orders/lib/sales-order-detail-model"

export function ReceivablePanel({
    order,
    selfReturn,
}: {
    order: SalesOrderDetailView
    selfReturn: string
}) {
    return (
        <div className="space-y-3">
            <SectionLead>
                回款收齐后系统自动结案。开票进度单独看，不挡结案。
            </SectionLead>
            <DocumentSection
                title="回款与开票"
                className="py-3 first:pt-0 last:pb-0"
                action={
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={receivableWorkspaceHref(
                                    order,
                                    selfReturn,
                                )}
                            />
                        }
                    >
                        <WalletIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        记一笔回款
                    </Button>
                }
            >
                <RelatedLanes
                    order={order}
                    selfReturn={selfReturn}
                    lanes={["receipt", "invoice"]}
                />
            </DocumentSection>
        </div>
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
            <SectionLead>
                改单会另开一笔，不会改掉客户正在执行的版本。生效前仍按当前版本履约和回款。
            </SectionLead>
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
