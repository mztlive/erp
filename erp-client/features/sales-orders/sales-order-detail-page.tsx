"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import {
    ArrowLeftIcon,
    BanIcon,
    FilePenLineIcon,
    ShieldAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    FormalActionResult,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useResolveProcurementRejectionMutation,
    useSalesOrderDetailQuery,
    useSalesOrderDraftResumeQuery,
    useStartSalesChangeOrderMutation,
} from "@/features/sales-orders/queries"
import { PROCUREMENT_REJECT_REASON_LABEL } from "@/features/sales-orders/labels"
import type { SalesOrderDetailView } from "@/features/sales-orders/api"
import { SalesOrderCreateForm } from "@/features/sales-orders/sales-order-create-page"
import { VoidSalesOrderDialog } from "@/features/sales-orders/void-sales-order-dialog"
import {
    CollaborationPanel,
    FocusTaskBanner,
    FulfillmentPanel,
    LifecycleRail,
    OverviewPanel,
    ReceivablePanel,
    SalesOrderIdentityHeader,
    VersionsPanel,
    navItemsFor,
} from "@/features/sales-orders/sales-order-detail-panels"
import {
    buildSelfHref,
    isActionableFocusTask,
    isOpenProcurementRejection,
    isSalesOrderDraft,
    isWorkSection,
    rejectionAllowsResubmit,
    rejectionAllowsVoid,
    resolveFocusTask,
    resolveNavSection,
    shouldOpenSalesOrderEditor,
    type NavSectionId,
    type WorkSectionId,
} from "@/features/sales-orders/sales-order-detail-model"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

export function SalesOrderDetailPage({
    salesOrderId,
    section,
}: {
    salesOrderId: string
    section?: string
}) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const returnTo = searchParams.get("returnTo")
    const fromWorkspace = searchParams.get("from")
    const pageMode = searchParams.get("mode")
    const query = useSalesOrderDetailQuery(salesOrderId)
    const profileQuery = useAccountProfileQuery()
    const changeMutation = useStartSalesChangeOrderMutation()
    const voidMutation = useResolveProcurementRejectionMutation()
    const [voidOpen, setVoidOpen] = React.useState(false)

    const [changeConfirmOpen, setChangeConfirmOpen] = React.useState(false)
    const [result, setResult] = React.useState<{
        status: "succeeded" | "blocked" | "rejected"
        title: string
        description: string
        reference: string
        nextResponsible?: string
    } | null>(null)

    const order = query.data
    const canAccept =
        order?.nature === "physical_service" &&
        order.allowedActions.includes("REGISTER_ACCEPTANCE")
    const canStartChange =
        order?.allowedActions.includes("START_SALES_CHANGE") ?? false
    const changeBlocker = order?.actionBlockers.find(
        (b) => b.action === "START_SALES_CHANGE",
    )

    const isCard = order?.nature === "card_voucher"
    const navSection = resolveNavSection(section, {
        from: fromWorkspace,
        isCard,
    })
    const acceptanceExpanded = section === "acceptance"
    const showApproval =
        Boolean(order?.activeCardSalesApproval) && section === "approval"
    const granted = profileQuery.data?.permissions
    const permissionKnown = !profileQuery.isPending
    const canResubmit = Boolean(
        order &&
        rejectionAllowsResubmit(order) &&
        permissionKnown &&
        hasPermission(granted, "sales_order:update"),
    )
    const canVoid = Boolean(
        order &&
        rejectionAllowsVoid(order) &&
        permissionKnown &&
        hasPermission(granted, "sales_order:delete"),
    )
    const showEditor = Boolean(
        order &&
        shouldOpenSalesOrderEditor({
            order,
            mode: pageMode,
            canResubmit:
                rejectionAllowsResubmit(order) &&
                (!permissionKnown ||
                    hasPermission(granted, "sales_order:update")),
        }),
    )

    const replaceOrderHref = React.useCallback(
        (patch: { section?: string; mode?: string | null }) => {
            const params = new URLSearchParams(searchParams.toString())
            if (patch.section) params.set("section", patch.section)
            if (patch.mode) params.set("mode", patch.mode)
            else if (patch.mode === null) params.delete("mode")
            const qs = params.toString()
            router.replace(
                qs
                    ? `/sales/orders/${salesOrderId}?${qs}`
                    : `/sales/orders/${salesOrderId}`,
                { scroll: false },
            )
        },
        [router, salesOrderId, searchParams],
    )

    const selectSection = React.useCallback(
        (next: NavSectionId | WorkSectionId | "versions") => {
            const params = new URLSearchParams()
            params.set("section", next)
            if (returnTo) params.set("returnTo", returnTo)
            if (fromWorkspace) params.set("from", fromWorkspace)
            const workItemId = searchParams.get("workItemId")
            if (
                workItemId &&
                (next === "approval" || next === "procurement-rejection")
            ) {
                params.set("workItemId", workItemId)
            }
            const queueContextId = searchParams.get("queueContextId")
            if (queueContextId && workItemId) {
                params.set("queueContextId", queueContextId)
            }
            const qs = params.toString()
            router.replace(
                qs
                    ? `/sales/orders/${salesOrderId}?${qs}`
                    : `/sales/orders/${salesOrderId}`,
                { scroll: false },
            )
        },
        [fromWorkspace, returnTo, router, salesOrderId, searchParams],
    )

    const enterRejectionEdit = React.useCallback(() => {
        replaceOrderHref({
            section: "procurement-rejection",
            mode: "edit",
        })
    }, [replaceOrderHref])

    const leaveRejectionEdit = React.useCallback(() => {
        replaceOrderHref({
            section: "procurement-rejection",
            mode: null,
        })
    }, [replaceOrderHref])

    React.useEffect(() => {
        if (!order || profileQuery.isPending) return
        if (pageMode !== "edit") return
        if (canResubmit || isSalesOrderDraft(order)) return
        replaceOrderHref({ mode: null })
    }, [canResubmit, order, pageMode, profileQuery.isPending, replaceOrderHref])

    if (query.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="销售单" description="正在加载详情…" />
                <div
                    className={cn(surfacePanelClassName, "h-72 animate-pulse")}
                    aria-busy="true"
                    aria-label="加载中"
                />
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <PageHeader title="销售单" />
                <BusinessFailureState
                    title="销售单加载失败"
                    error={query.error}
                    onRetry={() => {
                        void query.refetch()
                    }}
                />
            </PageScaffold>
        )
    }

    if (!order) {
        return (
            <PageScaffold>
                <PageHeader
                    title="销售单不存在"
                    description="未找到这张销售单。可能编号有误，或当前角色无权查看。"
                    actions={
                        <Button render={<Link href="/sales/orders" />}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const fromQueue =
        Boolean(returnTo) &&
        (fromWorkspace === "W07" ||
            fromWorkspace === "W08" ||
            fromWorkspace === "W09")
    const backHref = fromQueue && returnTo ? returnTo : "/sales/orders"
    const backLabel =
        fromWorkspace === "W07"
            ? "返回采购确认"
            : fromWorkspace === "W08"
              ? "返回采购单列表"
              : fromWorkspace === "W09"
                ? "返回履约处理"
                : "返回列表"

    const focusTask = resolveFocusTask(order, Boolean(canAccept))
    const actionableFocusTask = isActionableFocusTask(focusTask)
        ? focusTask
        : null
    const returnSection =
        section && section !== "overview" ? section : navSection
    const selfReturn = encodeURIComponent(
        buildSelfHref(order.id, returnSection, {
            returnTo,
            from: fromWorkspace,
        }),
    )
    const visibleNav = navItemsFor(order).filter((item) => item.show)

    const openRejection = isOpenProcurementRejection(order)
    const primaryTaskAction =
        openRejection && canResubmit ? (
            <Button type="button" size="sm" onClick={enterRejectionEdit}>
                改完再报
            </Button>
        ) : actionableFocusTask &&
          !(openRejection && navSection === "overview") ? (
            <Button
                type="button"
                size="sm"
                onClick={() => selectSection(actionableFocusTask.id)}
            >
                {actionableFocusTask.actionLabel}
            </Button>
        ) : null
    const bannerJump =
        focusTask &&
        !primaryTaskAction &&
        ((focusTask.id === "versions" && navSection !== "versions") ||
            (focusTask.id === "procurement-rejection" &&
                navSection !== "overview") ||
            (focusTask.id === "approval" && !showApproval) ||
            (focusTask.id === "acceptance" && !acceptanceExpanded))

    const handleVoidAfterRejection = async (reason: string) => {
        try {
            const outcome = await voidMutation.mutateAsync({
                salesOrderId: order.id,
                action: "VOID_AFTER_REJECTION",
                idempotencyKey: `pr-void-${order.id}-${Date.now()}`,
                voidReason: reason,
            })
            setResult({
                status: "rejected",
                title: "本单已作废",
                description: outcome.detail,
                reference: outcome.reference,
            })
        } catch (error) {
            const failure = getErrorPresentation(
                error,
                "作废未完成，请刷新后重试。",
            )
            setResult({
                status: "blocked",
                title: failure.title,
                description: failure.description,
                reference: order.documentNumber,
            })
            throw error
        }
    }

    if (showEditor) {
        return (
            <SalesOrderEditableCenter
                order={order}
                backHref={backHref}
                backLabel={backLabel}
                fromQueue={fromQueue}
                fromWorkspace={fromWorkspace}
                result={result}
                onResult={setResult}
                showBackToDetail={openRejection}
                onBackToDetail={leaveRejectionEdit}
                canVoidAfterRejection={canVoid}
            />
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "orders", label: "销售单", href: "/sales/orders" },
                    {
                        id: "detail",
                        label: order.documentNumber,
                        current: true,
                    },
                ]}
                metadata={
                    fromQueue ? (
                        <span>
                            {fromWorkspace === "W09"
                                ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                                : fromWorkspace === "W08"
                                  ? "从采购单打开 · 处理完可点返回，回到列表原位"
                                  : "从采购确认打开 · 处理完可点返回，回到列表原位"}
                        </span>
                    ) : undefined
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "back",
                                label: backLabel,
                                icon: ArrowLeftIcon,
                                variant: "outline",
                                render: <Link href={backHref} />,
                            },
                        ]}
                    />
                }
            />

            {focusTask ? (
                <FocusTaskBanner
                    order={order}
                    focusTask={focusTask}
                    canActOnRejection={canResubmit || canVoid}
                    action={
                        bannerJump ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => selectSection(focusTask.id)}
                            >
                                {focusTask.actionLabel}
                            </Button>
                        ) : undefined
                    }
                />
            ) : null}

            {result ? (
                <FormalActionResult
                    status={result.status}
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={[
                        {
                            label: "销售单",
                            value: order.documentNumber,
                        },
                        { label: "客户", value: order.customerName },
                        ...(result.nextResponsible
                            ? [
                                  {
                                      label: "下一步",
                                      value: result.nextResponsible,
                                  },
                              ]
                            : []),
                    ]}
                />
            ) : null}

            <SalesOrderIdentityHeader
                order={order}
                primaryAction={primaryTaskAction}
                secondaryActions={
                    <div className="flex flex-wrap items-center gap-2">
                        {openRejection && canVoid ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => setVoidOpen(true)}
                            >
                                <BanIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                作废
                            </Button>
                        ) : null}
                        {!openRejection ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={
                                    !canStartChange || changeMutation.isPending
                                }
                                title={
                                    !canStartChange
                                        ? (changeBlocker?.reason ??
                                          order.commercialReadOnlyReason ??
                                          "当前不能改单")
                                        : undefined
                                }
                                onClick={() => setChangeConfirmOpen(true)}
                            >
                                <FilePenLineIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                发起改单
                            </Button>
                        ) : null}
                    </div>
                }
            />

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <div className="border-b border-border/30 px-3 py-2 md:px-4">
                    <LifecycleRail order={order} />
                </div>

                <Tabs
                    className="gap-1"
                    value={navSection}
                    onValueChange={(next) => {
                        const target = next as NavSectionId
                        if (target !== navSection || isWorkSection(section))
                            selectSection(target)
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
                                                changeOnVersions
                                                    ? "warning"
                                                    : "info"
                                            }
                                            className="ml-1 h-5 px-1.5 text-2xs font-normal"
                                        >
                                            {changeOnVersions
                                                ? "改单中"
                                                : "待办"}
                                        </Badge>
                                    ) : null}
                                </TabsTrigger>
                            )
                        })}
                    </TabsList>

                    <TabsContent
                        value="overview"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <OverviewPanel
                            order={order}
                            showApproval={showApproval}
                            onApprovalResult={setResult}
                        />
                    </TabsContent>

                    <TabsContent
                        value="fulfillment"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <FulfillmentPanel
                            order={order}
                            selfReturn={selfReturn}
                            acceptanceExpanded={acceptanceExpanded}
                            canAccept={Boolean(canAccept)}
                            onExpandAcceptance={() =>
                                selectSection("acceptance")
                            }
                            onCollapseAcceptance={() =>
                                selectSection("fulfillment")
                            }
                        />
                    </TabsContent>

                    <TabsContent
                        value="receivable"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <ReceivablePanel
                            order={order}
                            selfReturn={selfReturn}
                        />
                    </TabsContent>

                    <TabsContent
                        value="collaboration"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <CollaborationPanel order={order} />
                    </TabsContent>

                    <TabsContent
                        value="versions"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <VersionsPanel order={order} />
                    </TabsContent>
                </Tabs>
            </div>

            <VoidSalesOrderDialog
                open={voidOpen}
                onOpenChange={setVoidOpen}
                pending={voidMutation.isPending}
                onConfirm={handleVoidAfterRejection}
            />

            <FormalActionConfirmDialog
                open={changeConfirmOpen}
                onOpenChange={setChangeConfirmOpen}
                title="发起改单"
                actionLabel="创建改单"
                confirmLabel="确认创建"
                fromStatus={{
                    label: `当前 v${order.version}`,
                    tone: "success",
                }}
                toStatus={{ label: "改单草稿", tone: "warning" }}
                lockedFields={["销售单号", "订单类型", "已生效版本"]}
                effects={[
                    "生成一笔改单，不改掉当前客户正在执行的版本",
                    "已有交付、回款、开票记录都会保留",
                    isCard
                        ? "卡券：运营确认影响 → 财务复核后新版本生效"
                        : "实物/服务：采购确认影响 → 财务复核后新版本生效",
                ]}
                nextDepartment={isCard ? "运营与财务" : "采购与财务"}
                onConfirm={async () => {
                    try {
                        const change = await changeMutation.mutateAsync({
                            salesOrderId: order.id,
                            baseRevisionNo: order.version,
                            nature: order.nature,
                        })
                        setResult({
                            status: "succeeded",
                            title: "改单已创建",
                            description: `已进入「${change.statusLabel}」。当前版本对客户仍然有效。`,
                            reference: change.id,
                            nextResponsible: isCard
                                ? "运营与财务"
                                : "采购与财务",
                        })
                    } catch (error) {
                        const failure = getErrorPresentation(
                            error,
                            "改单未创建，请刷新后重试。",
                        )
                        setResult({
                            status: "blocked",
                            title: failure.title,
                            description: failure.description,
                            reference: order.documentNumber,
                        })
                    }
                }}
            />
        </PageScaffold>
    )
}

function SalesOrderEditableCenter({
    order,
    backHref,
    backLabel,
    fromQueue,
    fromWorkspace,
    result,
    onResult,
    showBackToDetail = false,
    onBackToDetail,
    canVoidAfterRejection = false,
}: {
    order: SalesOrderDetailView
    backHref: string
    backLabel: string
    fromQueue: boolean
    fromWorkspace: string | null
    result: {
        status: "succeeded" | "blocked" | "rejected"
        title: string
        description: string
        reference: string
        nextResponsible?: string
    } | null
    onResult: (next: {
        status: "succeeded" | "blocked" | "rejected"
        title: string
        description: string
        reference: string
        nextResponsible?: string
    }) => void
    showBackToDetail?: boolean
    onBackToDetail?: () => void
    canVoidAfterRejection?: boolean
}) {
    const resumeQuery = useSalesOrderDraftResumeQuery(order.id)
    const voidMutation = useResolveProcurementRejectionMutation()
    const [voidOpen, setVoidOpen] = React.useState(false)
    const openRejection = isOpenProcurementRejection(order)
    const rejection = order.procurementRejection
    const canVoid = canVoidAfterRejection
    const reasonLabel = rejection
        ? (PROCUREMENT_REJECT_REASON_LABEL[rejection.rejectReasonCode] ??
          rejection.rejectReasonCode)
        : ""

    return (
        <PageScaffold className="pb-8">
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "orders", label: "销售单", href: "/sales/orders" },
                    {
                        id: "detail",
                        label: order.documentNumber,
                        current: true,
                    },
                ]}
                metadata={
                    fromQueue ? (
                        <span>
                            {fromWorkspace === "W09"
                                ? "从履约处理打开 · 处理完可点返回，回到列表原位"
                                : fromWorkspace === "W08"
                                  ? "从采购单打开 · 处理完可点返回，回到列表原位"
                                  : "从采购确认打开 · 处理完可点返回，回到列表原位"}
                        </span>
                    ) : undefined
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "back",
                                label: backLabel,
                                icon: ArrowLeftIcon,
                                variant: "outline",
                                render: <Link href={backHref} />,
                            },
                            ...(showBackToDetail && onBackToDetail
                                ? [
                                      {
                                          actionKey: "detail",
                                          label: "返回详情",
                                          variant: "outline" as const,
                                          onClick: onBackToDetail,
                                      },
                                  ]
                                : []),
                        ]}
                    />
                }
            />

            {openRejection && rejection ? (
                <Alert variant="warning" className="rounded-lg px-3 py-2">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle className="text-sm">采购未通过</AlertTitle>
                    <AlertDescription className="text-xs [&_p]:mb-0">
                        {[
                            reasonLabel,
                            rejection.rejectComment || null,
                            `${rejection.rejectedByLabel} · ${rejection.rejectedAt}`,
                            `第 ${rejection.rejectedSubmissionNo} 次报给采购`,
                            "改完整单后再报，或点「作废」",
                        ]
                            .filter(Boolean)
                            .join(" · ")}
                    </AlertDescription>
                </Alert>
            ) : null}

            {result ? (
                <FormalActionResult
                    status={result.status}
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={[
                        { label: "销售单", value: order.documentNumber },
                        { label: "客户", value: order.customerName },
                        ...(result.nextResponsible
                            ? [
                                  {
                                      label: "下一步",
                                      value: result.nextResponsible,
                                  },
                              ]
                            : []),
                    ]}
                />
            ) : null}

            <SalesOrderIdentityHeader
                order={order}
                secondaryActions={
                    openRejection && canVoid ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => setVoidOpen(true)}
                        >
                            <BanIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            作废
                        </Button>
                    ) : undefined
                }
            />

            {resumeQuery.isPending ? (
                <div
                    className={cn(surfacePanelClassName, "h-72 animate-pulse")}
                    aria-busy="true"
                    aria-label="正在加载可编辑内容"
                />
            ) : resumeQuery.isError || !resumeQuery.data ? (
                <BusinessFailureState
                    title="可编辑内容加载失败"
                    error={resumeQuery.error}
                    onRetry={() => {
                        void resumeQuery.refetch()
                    }}
                />
            ) : (
                <SalesOrderCreateForm
                    chrome="none"
                    purpose={openRejection ? "resubmit" : "draft"}
                    initialDraft={resumeQuery.data}
                    initialNature={resumeQuery.data.nature}
                    initialContractId={resumeQuery.data.contractId}
                    onResult={onResult}
                />
            )}

            <VoidSalesOrderDialog
                open={voidOpen}
                onOpenChange={setVoidOpen}
                pending={voidMutation.isPending}
                onConfirm={async (reason) => {
                    try {
                        const outcome = await voidMutation.mutateAsync({
                            salesOrderId: order.id,
                            action: "VOID_AFTER_REJECTION",
                            idempotencyKey: `pr-void-${order.id}-${Date.now()}`,
                            voidReason: reason,
                        })
                        onResult({
                            status: "rejected",
                            title: "本单已作废",
                            description: outcome.detail,
                            reference: outcome.reference,
                        })
                    } catch (error) {
                        const failure = getErrorPresentation(
                            error,
                            "作废未完成，请刷新后重试。",
                        )
                        onResult({
                            status: "blocked",
                            title: failure.title,
                            description: failure.description,
                            reference: order.documentNumber,
                        })
                        throw error
                    }
                }}
            />
        </PageScaffold>
    )
}
