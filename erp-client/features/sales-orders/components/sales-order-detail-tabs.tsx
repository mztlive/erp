"use client"

import { Badge } from "@/components/ui/badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    CollaborationPanel,
    FulfillmentPanel,
    OverviewPanel,
    ReceivablePanel,
    VersionsPanel,
} from "@/features/sales-orders/components/sales-order-detail-panels"
import {
    isWorkSection,
    type NavSectionId,
    type SalesOrderDetailActionResult,
    type WorkSectionId,
} from "@/features/sales-orders/lib/sales-order-detail-model"

export function SalesOrderDetailTabs({
    order,
    section,
    navSection,
    visibleNav,
    canAccept,
    acceptanceExpanded,
    showApproval,
    selfReturn,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onSelectSection,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    section?: string
    navSection: NavSectionId
    visibleNav: Array<{
        id: NavSectionId
        label: string
        hint: string
        show: boolean
    }>
    canAccept: boolean
    acceptanceExpanded: boolean
    showApproval: boolean
    selfReturn: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onSelectSection: (next: NavSectionId | WorkSectionId | "versions") => void
    onApprovalResult: (result: SalesOrderDetailActionResult) => void
}) {
    return (
        <Tabs
            className="gap-1"
            value={navSection}
            onValueChange={(next) => {
                const target = next as NavSectionId
                if (target !== navSection || isWorkSection(section))
                    onSelectSection(target)
            }}
        >
            <TabsList
                variant="line"
                className="sticky top-0 z-10 h-11 w-full justify-start gap-1 overflow-visible rounded-none border-b border-border/30 bg-card/95 px-3 group-data-horizontal/tabs:h-11 backdrop-blur supports-backdrop-filter:bg-card/80"
            >
                {visibleNav.map((item) => {
                    const todoOnFulfillment =
                        item.id === "fulfillment" && Boolean(canAccept)
                    const changeOnVersions =
                        item.id === "versions" &&
                        Boolean(order.activeChangeOrder)
                    return (
                        <TabsTrigger
                            key={item.id}
                            value={item.id}
                            title={item.hint}
                            className="h-full flex-none px-2 pb-2 leading-5 after:bottom-0"
                        >
                            {item.label}
                            {todoOnFulfillment || changeOnVersions ? (
                                <Badge
                                    variant={
                                        changeOnVersions ? "warning" : "info"
                                    }
                                    className="ml-1 h-5 px-1.5 text-2xs font-normal"
                                >
                                    {changeOnVersions ? "改单中" : "待办"}
                                </Badge>
                            ) : null}
                        </TabsTrigger>
                    )
                })}
            </TabsList>

            <TabsContent value="overview" className="px-3 pt-4 pb-4 md:px-4">
                <OverviewPanel
                    order={order}
                    showApproval={showApproval}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onApprovalResult={onApprovalResult}
                />
            </TabsContent>

            <TabsContent value="fulfillment" className="px-3 pt-4 pb-4 md:px-4">
                <FulfillmentPanel
                    order={order}
                    selfReturn={selfReturn}
                    acceptanceExpanded={acceptanceExpanded}
                    canAccept={Boolean(canAccept)}
                    onExpandAcceptance={() => onSelectSection("acceptance")}
                    onCollapseAcceptance={() => onSelectSection("fulfillment")}
                />
            </TabsContent>

            <TabsContent value="receivable" className="px-3 pt-4 pb-4 md:px-4">
                <ReceivablePanel order={order} selfReturn={selfReturn} />
            </TabsContent>

            <TabsContent
                value="collaboration"
                className="px-3 pt-4 pb-4 md:px-4"
            >
                <CollaborationPanel order={order} />
            </TabsContent>

            <TabsContent value="versions" className="px-3 pt-4 pb-4 md:px-4">
                <VersionsPanel order={order} />
            </TabsContent>
        </Tabs>
    )
}
