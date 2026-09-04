"use client"

import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import { RevisionHistoryCard } from "@/features/sales-orders/components/revision-history-card"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"

export { ReceivablePanel } from "@/features/sales-orders/components/sales-order-detail-receivable-panel"

export function VersionsPanel({
    order,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    return (
        <div className="space-y-6">
            {order.activeChangeOrder ? (
                <div className="space-y-2">
                    <SectionLead>
                        改单生效前，客户仍按当前版本执行。下面是进行中的改单，再往下是已经生效的历史版本。
                    </SectionLead>
                    <SalesChangeOrderApprovalSection
                        salesOrderId={order.id}
                        nature={order.nature}
                        changeOrder={order.activeChangeOrder}
                        onResult={onApprovalResult}
                    />
                </div>
            ) : (
                <SectionLead>
                    这里只看已经生效的销售版本。改单通过后会新增一版，旧版本仍可对照。
                </SectionLead>
            )}
            <RevisionHistoryCard
                revisions={order.revisions}
                currentVersion={order.currentRevisionNo}
                contractRevisionLabel={order.contractRevisionLabel}
            />
        </div>
    )
}

export function CollaborationPanel({ order }: { order: SalesOrderDetailView }) {
    void order;
    // TODO(商城重做): 执行投影恢复后替换此占位面板。
    return (
        <p className="text-sm text-muted-foreground">
            商城对接已移除，执行投影待未来重做时恢复。
        </p>
    )
}
