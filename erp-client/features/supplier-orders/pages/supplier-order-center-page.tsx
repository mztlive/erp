"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"
import { ExternalLinkIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
    detailWorkspaceStyles,
    GuardedBusinessAction,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
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
import {
    SECTION_LABEL,
    SECTIONS,
    type OrderSection,
} from "@/features/supplier-orders/types"
import { isNotFoundError } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"
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
    embedded = false,
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

    const { activeSection: urlSection, setSection: setUrlSection } =
        useSupplierOrderCenterSection(supplierOrderId, sectionProp)
    const [embeddedSection, setEmbeddedSection] = React.useState<OrderSection>(
        () => resolveSection(sectionProp),
    )
    const activeSection = embedded ? embeddedSection : urlSection
    const setSection = embedded ? setEmbeddedSection : setUrlSection

    const profileQuery = useAccountProfileQuery()
    const canDetail = hasPermission(
        profileQuery.data?.permissions,
        "supplier_fulfillment_order:detail",
    )

    const query = useSupplierOrderDetailQuery({
        orderId: supplierOrderId,
        workItemId,
        enabled: !profileQuery.isPending && canDetail,
    })
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

    const listBack = embedded
        ? undefined
        : {
              id: "supplier-order-center-header-back-list",
              label: "返回供应商订单列表",
              href: "/supplier-api/orders",
          }

    if (profileQuery.isPending || (canDetail && query.isPending)) {
        if (embedded) {
            return (
                <DetailWorkspaceSection>
                    <div
                        className="space-y-4"
                        aria-busy="true"
                        aria-label="正在加载供应商订单详情"
                    >
                        <div className="h-12 animate-pulse rounded-md bg-muted/70" />
                        <div className="h-44 animate-pulse rounded-md bg-muted/70" />
                    </div>
                </DetailWorkspaceSection>
            )
        }
        return (
            <DetailWorkspaceSkeleton
                title="供应商订单详情"
                description="正在读取履约进度、商品明细与结算信息…"
                sidebar
            />
        )
    }

    if (profileQuery.isError) {
        const content = (
            <DetailWorkspaceSection width="form">
                <BusinessFailureState
                    idPrefix="supplier-order-center-profile"
                    title="账号权限读取失败"
                    error={profileQuery.error}
                    onRetry={() => void profileQuery.refetch()}
                />
            </DetailWorkspaceSection>
        )
        if (embedded) return content
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" back={listBack} />
                <DetailWorkspaceBody>{content}</DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!canDetail) {
        const content = (
            <DetailWorkspaceSection width="form">
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号没有查看供应商订单详情的权限，请联系管理员开通。"
                />
            </DetailWorkspaceSection>
        )
        if (embedded) return content
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" back={listBack} />
                <DetailWorkspaceBody>{content}</DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (query.isError) {
        const notFound = isNotFoundError(query.error)
        const content = (
            <DetailWorkspaceSection width="form">
                {notFound ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="未找到供应商订单"
                        description="该供应商订单不存在或已归档，请返回列表重新选择。"
                        action={
                            !embedded ? (
                                <Button
                                    id="supplier-order-center-not-found-back"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    render={
                                        <Link href="/supplier-api/orders" />
                                    }
                                >
                                    返回供应商订单列表
                                </Button>
                            ) : undefined
                        }
                    />
                ) : (
                    <BusinessFailureState
                        idPrefix="supplier-order-center-error"
                        title="供应商订单加载失败"
                        error={query.error}
                        action={
                            <Button
                                id="supplier-order-center-error-retry"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void query.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                )}
            </DetailWorkspaceSection>
        )
        if (embedded) return content
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" back={listBack} />
                <DetailWorkspaceBody>{content}</DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!detail) {
        const content = (
            <DetailWorkspaceSection width="form">
                <BusinessEmptyState
                    kind="no-data"
                    title="未找到供应商订单"
                    description="该订单不存在或当前角色无权访问。"
                    action={
                        !embedded ? (
                            <Button
                                id="supplier-order-center-not-found-back"
                                type="button"
                                variant="outline"
                                size="sm"
                                render={<Link href="/supplier-api/orders" />}
                            >
                                返回供应商订单列表
                            </Button>
                        ) : undefined
                    }
                />
            </DetailWorkspaceSection>
        )
        if (embedded) return content
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商订单" back={listBack} />
                <DetailWorkspaceBody>{content}</DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const o = detail.order
    const canEditExecution = hasPermission(
        profileQuery.data?.permissions,
        "supplier_fulfillment_order:submit",
    )

    const sectionTabs = (
        <DetailWorkspaceTabs
            value={activeSection}
            onValueChange={(value) => setSection(resolveSection(value))}
            items={SECTIONS.map((id) => ({
                id,
                label: SECTION_LABEL[id],
            }))}
            ariaLabel="供应商订单分区"
            tabIdPrefix="supplier-order-center-tab"
        />
    )

    const alertsContent = (
        <>
            <WorkItemProcessPanel
                workItem={detail.workItem}
                workItemBlocker={detail.workItemBlocker}
                responsibilityStatus={derivation.responsibilityStatus}
                canCompleteTask={derivation.canCompleteTask}
                pending={completeTaskMutation.isPending}
                onProcess={() => task.setCompleteOpen(true)}
            />

            <StatusAlertsPanel
                order={o}
                lastInvestigation={detail.lastInvestigation}
                noQueryCapability={derivation.noQueryCapability}
                trackingError={detail.trackingError}
            />

            {result ? (
                <ResultPanel
                    result={result}
                    order={o}
                    costs={detail.costs}
                    onClose={() => setResult(null)}
                />
            ) : null}
        </>
    )

    const activeSectionContent = (
        <>
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
                    <SupplierOrderExecutionProgress orderId={supplierOrderId} />
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
        </>
    )

    const dialogs = (
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
    )

    if (embedded) {
        return (
            <div className="min-w-0 [&>[data-slot=detail-workspace-section]+[data-slot=detail-workspace-section]]:border-t [&>[data-slot=detail-workspace-section]+[data-slot=detail-workspace-section]]:border-border/60">
                <DetailWorkspaceSection className="py-4 first:pt-4">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="min-w-0 space-y-2">
                            <div
                                role="list"
                                aria-label="供应商订单业务状态"
                                className="flex flex-wrap items-center gap-x-4 gap-y-1.5"
                            >
                                <div
                                    role="listitem"
                                    className="flex items-center gap-1.5"
                                >
                                    <span className="text-xs text-muted-foreground">
                                        履约
                                    </span>
                                    <StatusBadge
                                        label={o.fulfillmentLabel}
                                        tone={o.fulfillmentTone}
                                    />
                                </div>
                                <div
                                    role="listitem"
                                    className="flex items-center gap-1.5"
                                >
                                    <span className="text-xs text-muted-foreground">
                                        取消
                                    </span>
                                    <StatusBadge
                                        label={o.cancelLabel}
                                        tone={o.cancelTone}
                                    />
                                </div>
                                <div
                                    role="listitem"
                                    className="flex items-center gap-1.5"
                                >
                                    <span className="text-xs text-muted-foreground">
                                        退款
                                    </span>
                                    <StatusBadge
                                        label={o.refundLabel}
                                        tone={o.refundTone}
                                    />
                                </div>
                            </div>
                            <DetailWorkspaceMeta
                                items={[
                                    <span key="external" className="num">
                                        供应商单号{" "}
                                        {o.externalOrderNo ?? "尚未返回"}
                                    </span>,
                                    o.connectionCode ? (
                                        <span key="conn" className="num">
                                            连接 {o.connectionCode}
                                        </span>
                                    ) : null,
                                    o.createdAt ? (
                                        <span key="created" className="num">
                                            创建于{" "}
                                            {formatDateTime(
                                                o.createdAt,
                                                "fullIntl",
                                                "passthrough",
                                            )}
                                        </span>
                                    ) : null,
                                ]}
                            />
                        </div>

                        {derivation.isResultUnknown ||
                        detail.allowedActions.includes("ESCALATE_W29") ? (
                            <div className="flex shrink-0 flex-wrap items-center gap-2">
                                {derivation.isResultUnknown ? (
                                    <PermissionGuard permission="supplier_fulfillment_order:investigate">
                                        <GuardedBusinessAction
                                            id="supplier-order-center-header-replay"
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={
                                                !derivation.canReplay ||
                                                replayMutation.isPending
                                            }
                                            reason={
                                                derivation.canReplay
                                                    ? "已确认无结果，可安全重发"
                                                    : (detail.actionBlockers.find(
                                                          (b) =>
                                                              b.action ===
                                                              "REPLAY",
                                                      )?.message ??
                                                      "需先查询确认无结果后，方可重发")
                                            }
                                            onClick={() => {
                                                if (derivation.canReplay) {
                                                    actions.setReplayOpen(true)
                                                }
                                            }}
                                        >
                                            安全重发
                                        </GuardedBusinessAction>
                                        <GuardedBusinessAction
                                            id="supplier-order-center-header-query-result"
                                            loading={
                                                queryResultMutation.isPending
                                            }
                                            type="button"
                                            size="sm"
                                            disabled={
                                                !derivation.canQuery ||
                                                queryResultMutation.isPending
                                            }
                                            reason={
                                                !derivation.canQuery
                                                    ? (detail.actionBlockers.find(
                                                          (b) =>
                                                              b.action ===
                                                              "QUERY_RESULT",
                                                      )?.message ??
                                                      "当前不可查询")
                                                    : undefined
                                            }
                                            onClick={() =>
                                                void actions.handleQueryResult()
                                            }
                                        >
                                            {queryResultMutation.isPending
                                                ? "查询中…"
                                                : "查询原下单结果"}
                                        </GuardedBusinessAction>
                                    </PermissionGuard>
                                ) : null}
                                {detail.allowedActions.includes(
                                    "ESCALATE_W29",
                                ) ? (
                                    <PermissionGuard permission="integration_error_task:list">
                                        <Button
                                            id="supplier-order-center-header-escalate"
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                            render={
                                                <Link href="/governance/integration-errors" />
                                            }
                                        >
                                            转接口错误中心
                                            <ExternalLinkIcon
                                                data-icon="inline-end"
                                                aria-hidden="true"
                                            />
                                        </Button>
                                    </PermissionGuard>
                                ) : null}
                            </div>
                        ) : null}
                    </div>

                    {alertsContent}
                </DetailWorkspaceSection>

                <div className="border-b border-border/60">{sectionTabs}</div>

                {activeSectionContent}

                <SupplierOrderExecutionSetup
                    orderId={supplierOrderId}
                    addressSummary={
                        detail.address.revealed ?? detail.address.masked
                    }
                    executionNotice={o.executionNotice}
                    canEdit={canEditExecution}
                    asSection
                />

                {dialogs}
            </div>
        )
    }

    const hasAlertContent = Boolean(
        detail.workItem ||
        detail.workItemBlocker ||
        detail.trackingError ||
        derivation.isResultUnknown ||
        (o.fulfillmentStatus === "COMPLETED" && o.refundStatus === "PARTIAL") ||
        result,
    )

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
                embedded={false}
                tabs={sectionTabs}
            />
            <DetailWorkspaceBody
                sidebarLabel="执行配置与协同任务"
                sidebar={
                    <div className="space-y-5">
                        <SupplierOrderExecutionSetup
                            orderId={supplierOrderId}
                            addressSummary={
                                detail.address.revealed ?? detail.address.masked
                            }
                            executionNotice={o.executionNotice}
                            canEdit={canEditExecution}
                        />
                        {detail.workItem ? (
                            <WorkItemCard
                                workItem={detail.workItem}
                                orderNo={o.orderNo}
                            />
                        ) : null}
                    </div>
                }
                alerts={hasAlertContent ? alertsContent : undefined}
            >
                {activeSectionContent}
            </DetailWorkspaceBody>

            {dialogs}
        </DetailWorkspacePage>
    )
}
