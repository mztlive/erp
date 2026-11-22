"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceTabs,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
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
import { hasPermission } from "@/lib/permissions"
import { SupplierOrderExecutionSetup } from "../components/supplier-order-execution-setup"
import { SupplierOrderExecutionProgress } from "../components/supplier-order-execution-progress"
import { SupplierOrderCenterDialogs } from "@/features/supplier-orders/components/supplier-order-preview-center-dialogs"
import {
    AftersalesSection,
    CostsSection,
} from "@/features/supplier-orders/components/supplier-order-preview-center-aftersales-costs"
import { AuditSection } from "@/features/supplier-orders/components/supplier-order-preview-center-audit"
import { FulfillmentSection } from "@/features/supplier-orders/components/supplier-order-preview-center-fulfillment"
import { SupplierOrderCenterHeader } from "@/features/supplier-orders/components/supplier-order-preview-center-header"
import {
    ItemsSection,
    OverviewSection,
} from "@/features/supplier-orders/components/supplier-order-preview-center-overview-items"
import {
    ResultPanel,
    StatusAlertsPanel,
    WorkItemCard,
    WorkItemProcessPanel,
} from "@/features/supplier-orders/components/supplier-order-preview-center-panels"
import {
    useSupplierOrderCenterActions,
    useSupplierOrderCenterResult,
} from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"
import { useSupplierOrderCenterOrderActions } from "@/features/supplier-orders/hooks/use-supplier-order-center-order-actions"
import { useSupplierOrderCenterDerivation } from "@/features/supplier-orders/hooks/use-supplier-order-center-derivation"
import { useSupplierOrderCenterCommandIdentity } from "@/features/supplier-orders/hooks/use-supplier-order-center-identity"
import {
    resolveSection,
    useSupplierOrderCenterSection,
} from "@/features/supplier-orders/hooks/use-supplier-order-center-section"
import { useSupplierOrderCenterTaskActions } from "@/features/supplier-orders/hooks/use-supplier-order-center-task-actions"

export function SupplierOrderCenterPage({
    supplierOrderId,
    section: sectionProp,
    workItemId: workItemIdProp,
    onTaskCompleted,
}: {
    supplierOrderId: string
    section?: string
    workItemId?: string
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
}) {
    const searchParams = useSearchParams()
    const workItemId =
        workItemIdProp ?? searchParams.get("workItemId") ?? undefined

    const { activeSection, setSection } = useSupplierOrderCenterSection(
        supplierOrderId,
        sectionProp,
    )

    const query = useSupplierOrderDetailQuery({
        orderId: supplierOrderId,
        workItemId,
    })
    const profileQuery = useAccountProfileQuery()
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
        completeTaskMutation,
        commandIdentity: identity.commandIdentity,
        forgetCommandIdentity: identity.forgetCommandIdentity,
        onTaskCompleted,
    })

    React.useEffect(() => {
        return () => {
            void clearAddressReveal(supplierOrderId)
        }
    }, [supplierOrderId])

    if (query.isPending) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <div aria-busy="true" className="flex flex-col gap-4">
                            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                            <div className="h-28 animate-pulse rounded-lg bg-muted" />
                            <div className="h-64 animate-pulse rounded-lg bg-muted" />
                        </div>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (query.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="供应商订单加载失败"
                            error={query.error}
                            action={
                                <Button
                                    id="supplier-order-center-error-retry"
                                    type="button"
                                    onClick={() => void query.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!detail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <Alert variant="warning">
                            <AlertTitle>未找到供应商订单</AlertTitle>
                            <AlertDescription>
                                该订单不存在或当前角色无权访问。
                                <Button
                                    id="supplier-order-center-not-found-back"
                                    type="button"
                                    variant="link"
                                    className="px-1"
                                    render={
                                        <Link href="/supplier-api/orders" />
                                    }
                                >
                                    返回列表
                                </Button>
                            </AlertDescription>
                        </Alert>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const o = detail.order

    return (
        <DetailWorkspacePage>
            <SupplierOrderCenterHeader
                order={o}
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
                tabs={
                    <DetailWorkspaceTabs
                        value={activeSection}
                        onValueChange={(value) =>
                            setSection(resolveSection(value))
                        }
                        items={SECTIONS.map((id) => ({
                            id,
                            label: SECTION_LABEL[id],
                        }))}
                        ariaLabel="供应商订单分区"
                        tabIdPrefix="supplier-order-center-tab"
                    />
                }
            />
            <DetailWorkspaceBody
                alerts={
                    <>
                        <WorkItemProcessPanel
                            workItem={detail.workItem}
                            workItemBlocker={detail.workItemBlocker}
                            responsibilityStatus={
                                derivation.responsibilityStatus
                            }
                            canCompleteTask={derivation.canCompleteTask}
                            pending={completeTaskMutation.isPending}
                            onProcess={() => task.setCompleteOpen(true)}
                        />

                        <SupplierOrderExecutionSetup
                            orderId={supplierOrderId}
                            addressSummary={
                                detail.address.revealed ?? detail.address.masked
                            }
                            canEdit={hasPermission(
                                profileQuery.data?.permissions,
                                "supplier_fulfillment_order:submit",
                            )}
                        />

                        <StatusAlertsPanel
                            order={o}
                            lastInvestigation={detail.lastInvestigation}
                            noQueryCapability={derivation.noQueryCapability}
                            trackingError={detail.trackingError}
                        />

                        {detail.workItem ? (
                            <WorkItemCard
                                workItem={detail.workItem}
                                orderNo={o.orderNo}
                            />
                        ) : null}

                        {result ? (
                            <ResultPanel
                                result={result}
                                order={o}
                                costs={detail.costs}
                                onClose={() => setResult(null)}
                            />
                        ) : null}
                    </>
                }
            >
                {activeSection === "overview" ? (
                    <OverviewSection
                        order={o}
                        purchaseOrderId={detail.purchaseOrderId}
                        purchaseOrderNo={detail.purchaseOrderNo}
                    />
                ) : null}

                {activeSection === "items" ? (
                    <ItemsSection
                        items={detail.items}
                        totalQuantity={derivation.totalQuantity}
                        totalCostGross={derivation.totalCostGross}
                    />
                ) : null}

                {activeSection === "fulfillment" ? (
                    <>
                        <DetailWorkspaceSection>
                            <SupplierOrderExecutionProgress
                                orderId={supplierOrderId}
                            />
                        </DetailWorkspaceSection>
                        <FulfillmentSection
                            logistics={detail.logistics}
                            address={detail.address}
                            statusHistory={detail.statusHistory}
                            canReveal={derivation.canReveal}
                            revealPending={revealMutation.isPending}
                            onReveal={() => void orderActions.handleReveal()}
                            onHide={() => {
                                void clearAddressReveal(supplierOrderId).then(
                                    () => void query.refetch(),
                                )
                            }}
                        />
                    </>
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
            </DetailWorkspaceBody>

            <SupplierOrderCenterDialogs
                order={o}
                taskVersion={detail.workItem?.taskVersion}
                completionEvidence={derivation.completionEvidence}
                replayOpen={actions.replayOpen}
                onReplayOpenChange={actions.setReplayOpen}
                replayPending={replayMutation.isPending}
                onReplayConfirm={() => actions.handleReplay()}
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
        </DetailWorkspacePage>
    )
}
