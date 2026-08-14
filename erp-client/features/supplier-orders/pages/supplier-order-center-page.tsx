"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"

import { BusinessFailureState, PageScaffold, surfacePanelClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import { clearAddressReveal } from "@/features/supplier-orders/api/index"
import {
    useAddNoteMutation,
    useAfterSalesActionMutation,
    useCompleteOrderTaskMutation,
    useQueryResultMutation,
    useReplayOrderMutation,
    useRevealAddressMutation,
    useSupplierOrderDetailQuery,
} from "@/features/supplier-orders/hooks/queries"
import { SECTION_LABEL, SECTIONS } from "@/features/supplier-orders/types"
import { cn } from "@/lib/utils"
import { SupplierOrderCenterDialogs } from "@/features/supplier-orders/components/supplier-order-preview-center-dialogs"
import { AftersalesSection, CostsSection } from "@/features/supplier-orders/components/supplier-order-preview-center-aftersales-costs"
import { AuditSection } from "@/features/supplier-orders/components/supplier-order-preview-center-audit"
import { FulfillmentSection } from "@/features/supplier-orders/components/supplier-order-preview-center-fulfillment"
import { SupplierOrderCenterHeader } from "@/features/supplier-orders/components/supplier-order-preview-center-header"
import { ItemsSection, OverviewSection } from "@/features/supplier-orders/components/supplier-order-preview-center-overview-items"
import { ResultPanel, StatusAlertsPanel, WorkItemCard, WorkItemProcessPanel } from "@/features/supplier-orders/components/supplier-order-preview-center-panels"
import { useSupplierOrderCenterActions, useSupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"
import { useSupplierOrderCenterOrderActions } from "@/features/supplier-orders/hooks/use-supplier-order-center-order-actions"
import { useSupplierOrderCenterDerivation } from "@/features/supplier-orders/hooks/use-supplier-order-center-derivation"
import { useSupplierOrderCenterReleaseForm } from "@/features/supplier-orders/hooks/use-supplier-order-center-forms"
import { useSupplierOrderCenterCommandIdentity } from "@/features/supplier-orders/hooks/use-supplier-order-center-identity"
import { resolveSection, useSupplierOrderCenterSection } from "@/features/supplier-orders/hooks/use-supplier-order-center-section"
import { useSupplierOrderCenterTaskActions } from "@/features/supplier-orders/hooks/use-supplier-order-center-task-actions"

export function SupplierOrderCenterPage({
    supplierOrderId,
    section: sectionProp,
}: {
    supplierOrderId: string
    section?: string
}) {
    const searchParams = useSearchParams()
    const from = searchParams.get("from")
    const sourceId = searchParams.get("sourceId")
    const workItemId = searchParams.get("workItemId") ?? undefined

    const { activeSection, setSection } = useSupplierOrderCenterSection(
        supplierOrderId,
        sectionProp,
    )

    const query = useSupplierOrderDetailQuery({
        orderId: supplierOrderId,
        workItemId,
    })
    const profileQuery = useAccountProfileQuery()
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const queryResultMutation = useQueryResultMutation()
    const replayMutation = useReplayOrderMutation()
    const completeTaskMutation = useCompleteOrderTaskMutation()
    const afterSalesMutation = useAfterSalesActionMutation()
    const revealMutation = useRevealAddressMutation()
    const noteMutation = useAddNoteMutation()

    const identity = useSupplierOrderCenterCommandIdentity()
    const { result, setResult } = useSupplierOrderCenterResult()

    const detail = query.data

    const actions = useSupplierOrderCenterActions({
        orderId: supplierOrderId,
        workItemId,
        detail,
        currentUserId: profileQuery.data?.userid,
        setResult,
        queryResultMutation,
        replayMutation,
        commandIdentity: identity.commandIdentity,
        forgetCommandIdentity: identity.forgetCommandIdentity,
    })

    const orderActions = useSupplierOrderCenterOrderActions({
        orderId: supplierOrderId,
        detail,
        setResult,
        afterSalesMutation,
        revealMutation,
    })

    const derivation = useSupplierOrderCenterDerivation({
        detail,
        currentUserId: profileQuery.data?.userid,
        latestInvestigation: actions.latestInvestigation,
    })

    const task = useSupplierOrderCenterTaskActions({
        detail,
        completionEvidence: derivation.completionEvidence,
        refetch: () => query.refetch(),
        setResult,
        responsibilityMutation,
        completeTaskMutation,
        commandIdentity: identity.commandIdentity,
        forgetCommandIdentity: identity.forgetCommandIdentity,
    })

    const release = useSupplierOrderCenterReleaseForm({
        detail,
        setResult,
        responsibilityMutation,
        refetch: () => query.refetch(),
        commandIdentity: identity.commandIdentity,
        forgetCommandIdentity: identity.forgetCommandIdentity,
    })

    const titleRef = React.useRef<HTMLSpanElement>(null)

    React.useEffect(() => {
        titleRef.current?.focus()
    }, [supplierOrderId, activeSection])

    React.useEffect(() => {
        return () => {
            void clearAddressReveal(supplierOrderId)
        }
    }, [supplierOrderId])

    if (query.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-28 animate-pulse rounded-lg bg-muted" />
                <div className="h-64 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="供应商订单加载失败"
                    error={query.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!detail) {
        return (
            <PageScaffold>
                <Alert variant="warning">
                    <AlertTitle>未找到供应商订单</AlertTitle>
                    <AlertDescription>
                        该订单不存在或当前角色无权访问。
                        <Button
                            type="button"
                            variant="link"
                            className="px-1"
                            render={<Link href="/supplier-api/orders" />}
                        >
                            返回列表
                        </Button>
                    </AlertDescription>
                </Alert>
            </PageScaffold>
        )
    }

    const o = detail.order

    return (
        <PageScaffold>
            <SupplierOrderCenterHeader
                order={o}
                from={from}
                sourceId={sourceId}
                titleRef={titleRef}
                canQuery={derivation.canQuery}
                canReplay={derivation.canReplay}
                isResultUnknown={derivation.isResultUnknown}
                actionBlockers={detail.actionBlockers}
                allowedActions={detail.allowedActions}
                queryPending={queryResultMutation.isPending}
                replayPending={replayMutation.isPending}
                onQueryResult={() => void actions.handleQueryResult()}
                onReplayClick={() => {
                    if (derivation.canReplay) actions.setReplayOpen(true)
                }}
            />

            <WorkItemProcessPanel
                workItem={detail.workItem}
                workItemBlocker={detail.workItemBlocker}
                responsibilityStatus={derivation.responsibilityStatus}
                canCompleteTask={derivation.canCompleteTask}
                pending={
                    responsibilityMutation.isPending ||
                    completeTaskMutation.isPending
                }
                releasePending={responsibilityMutation.isPending}
                onStartProcessing={() => void task.handleStartProcessing()}
                onProcess={() => task.setCompleteOpen(true)}
                onRelease={() => release.setReleaseOpen(true)}
            />

            <StatusAlertsPanel
                order={o}
                lastInvestigation={detail.lastInvestigation}
                noQueryCapability={derivation.noQueryCapability}
            />

            {detail.workItem ? (
                <WorkItemCard workItem={detail.workItem} orderNo={o.orderNo} />
            ) : null}

            {result ? (
                <ResultPanel
                    result={result}
                    order={o}
                    costs={detail.costs}
                    onClose={() => setResult(null)}
                />
            ) : null}

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={activeSection}
                    onValueChange={(v) => setSection(resolveSection(v))}
                >
                    <TabsList
                        variant="line"
                        className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {SECTIONS.map((s) => (
                            <TabsTrigger key={s} value={s}>
                                {SECTION_LABEL[s]}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>

                <div className="space-y-4 p-3 md:p-4">
                    {activeSection === "overview" ? (
                        <OverviewSection order={o} />
                    ) : null}

                    {activeSection === "items" ? (
                        <ItemsSection
                            items={detail.items}
                            totalQuantity={derivation.totalQuantity}
                            totalCostGross={derivation.totalCostGross}
                        />
                    ) : null}

                    {activeSection === "fulfillment" ? (
                        <FulfillmentSection
                            logistics={detail.logistics}
                            address={detail.address}
                            statusHistory={detail.statusHistory}
                            canReveal={derivation.canReveal}
                            revealPending={revealMutation.isPending}
                            onReveal={() =>
                                void orderActions.handleReveal()
                            }
                            onHide={() => {
                                void clearAddressReveal(supplierOrderId).then(
                                    () => void query.refetch(),
                                )
                            }}
                        />
                    ) : null}

                    {activeSection === "aftersales" ? (
                        <AftersalesSection
                            afterSales={detail.afterSales}
                            pending={afterSalesMutation.isPending}
                            onRequest={orderActions.setAfterSalesConfirm}
                        />
                    ) : null}

                    {activeSection === "costs" ? (
                        <CostsSection costs={detail.costs} />
                    ) : null}

                    {activeSection === "audit" ? (
                        <AuditSection
                            orderId={supplierOrderId}
                            detail={detail}
                            noteMutation={noteMutation}
                            setResult={setResult}
                        />
                    ) : null}
                </div>
            </div>

            <SupplierOrderCenterDialogs
                order={o}
                taskVersion={detail.workItem?.taskVersion}
                completionEvidence={derivation.completionEvidence}
                replayOpen={actions.replayOpen}
                onReplayOpenChange={actions.setReplayOpen}
                replayPending={replayMutation.isPending}
                onReplayConfirm={() => actions.handleReplay()}
                releaseOpen={release.releaseOpen}
                onReleaseOpenChange={release.setReleaseOpen}
                releaseForm={release.releaseForm}
                completeOpen={task.completeOpen}
                onCompleteOpenChange={task.setCompleteOpen}
                completePending={completeTaskMutation.isPending}
                onCompleteConfirm={() => task.handleCompleteTask()}
                afterSalesRequest={orderActions.afterSalesConfirm}
                onAfterSalesRequestChange={orderActions.setAfterSalesConfirm}
                afterSalesPending={afterSalesMutation.isPending}
                onAfterSalesConfirm={() => {
                    const request = orderActions.afterSalesConfirm
                    if (request) {
                        return orderActions.handleAfterSales(
                            request.action,
                            request.requestId,
                        )
                    }
                }}
            />
        </PageScaffold>
    )
}
