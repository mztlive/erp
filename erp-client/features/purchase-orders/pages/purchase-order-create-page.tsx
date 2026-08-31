"use client"

import * as React from "react"
import type { ReactNode } from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useStore } from "@tanstack/react-form"
import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    MoneyValue,
    PageActions,
    PageHeader,
    PageScaffold,
    StickyTotalBar,
    WorkspaceTaskFooter,
    surfaceClassName,
    workspaceEmbeddedScaffoldClassName,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { toast } from "@/components/ui/toast"
import { PurchaseOrderCreateBatchBar } from "@/features/purchase-orders/components/purchase-order-create-batch-bar"
import { PurchaseOrderCreatePreviewDialog } from "@/features/purchase-orders/components/purchase-order-create-preview"
import { PurchaseOrderCreateSourcingTable } from "@/features/purchase-orders/components/purchase-order-create-sourcing-table"
import { PurchaseOrderCreateSourcePanel } from "@/features/purchase-orders/components/purchase-order-create-source-panel"
import {
    useCreateFromSourcingMutation,
    useCreationBasesQuery,
} from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderCreateFormApi } from "@/features/purchase-orders/lib/purchase-order-create-form-types"
import {
    assignBestSourcingOptions,
    buildDefaultSourcingLines,
    buildPurchaseOrderPreviews,
    buildStockAllocationPreviews,
    buildSourcingWorkspace,
    commonSourcingOptionsForSelected,
    findSourcingOption,
    sourcingFormLinesReady,
    sumPreviewTotals,
    type SourcingLineInput,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    collectSourcingErrorMessages,
    sourcingFormValidationError,
} from "@/features/purchase-orders/lib/purchase-order-create-validation"
import { cn } from "@/lib/utils"

/**
 * 新建采购单页面：按销售明细选源，预览拆单结果后确认创建并提交审批；成功后回到列表。
 */
export type SalesOrderPaperPreviewRenderProps = Readonly<{
    salesOrderId: string | null
    title?: string
    open: boolean
    onOpenChange: (open: boolean) => void
}>

export function PurchaseOrderCreatePage({
    initialSalesOrderId = "",
    initialWorkItemId = "",
    embedded = false,
    onTaskCompleted,
    renderSalesOrderPreview,
}: {
    initialSalesOrderId?: string
    initialWorkItemId?: string
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
    /** 按销售单身份渲染原始单据预览；不得用供给投影凑纸。 */
    renderSalesOrderPreview?: (
        props: SalesOrderPaperPreviewRenderProps,
    ) => ReactNode
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const salesOrderReturnTo = embedded
        ? `${pathname}${searchParams.toString() ? `?${searchParams}` : ""}`
        : undefined
    const basesQuery = useCreationBasesQuery({
        salesOrderId: initialSalesOrderId || undefined,
        workItemId: initialWorkItemId || undefined,
    })
    const createMutation = useCreateFromSourcingMutation()
    const [previewOpen, setPreviewOpen] = React.useState(false)
    const [sourcePaper, setSourcePaper] = React.useState<{
        id: string
        title: string
    } | null>(null)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [actionError, setActionError] = React.useState<{
        title: string
        description: string
    } | null>(null)
    const createIntentRef = React.useRef<{
        fingerprint: string
        idempotencyKey: string
    } | null>(null)
    const submittingFromConfirmRef = React.useRef(false)

    const workspace = React.useMemo(
        () => buildSourcingWorkspace(basesQuery.data ?? []),
        [basesQuery.data],
    )
    const workspaceRef = React.useRef(workspace)
    workspaceRef.current = workspace

    const form = useAppForm({
        defaultValues: {
            salesOrderId: initialSalesOrderId,
            lines: [] as SourcingLineInput[],
        },
        validators: {
            onChange: ({ value }) => {
                const order = workspaceRef.current.find(
                    (candidate) =>
                        candidate.salesOrderId === value.salesOrderId,
                )
                return sourcingFormValidationError(order, value)
            },
        },
        onSubmit: async ({ value }) => {
            const order = workspaceRef.current.find(
                (candidate) => candidate.salesOrderId === value.salesOrderId,
            )
            if (!order) return
            const selected = value.lines.flatMap((line) => {
                if (!line.selected || !line.basisId) return []
                const product = order.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, line.basisId)
                if (!option) return []
                return [{ line, option }]
            })
            const fingerprint = JSON.stringify({
                salesOrderId: order.salesOrderId,
                workItemId: order.workItemId,
                lines: selected.map(({ line, option }) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    basisId: line.basisId,
                    sourceType: option.sourceType,
                    targetWarehouseId: line.targetWarehouseId || undefined,
                    quantity: line.quantity.trim(),
                    expectedDeliveryDate: line.expectedDeliveryDate,
                })),
            })
            if (createIntentRef.current?.fingerprint !== fingerprint) {
                createIntentRef.current = {
                    fingerprint,
                    idempotencyKey: `create-sourcing:${order.salesOrderId}:${crypto.randomUUID()}`,
                }
            }
            const result = await createMutation.mutateAsync({
                workItemId: order.workItemId,
                salesOrderId: order.salesOrderId,
                lines: selected.map(({ line, option }) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    basisId: line.basisId,
                    sourceType: option.sourceType,
                    targetWarehouseId: line.targetWarehouseId || undefined,
                    quantity: line.quantity.trim(),
                    expectedDeliveryDate: line.expectedDeliveryDate,
                })),
                idempotencyKey: createIntentRef.current.idempotencyKey,
            })
            if (result.status === "succeeded") {
                createIntentRef.current = null
                const count = result.data.orders.length
                const stockCount = result.data.stockReservations.length
                const taskCompleted = result.data.workItemStatus === "COMPLETED"
                toast.add({
                    title: taskCompleted
                        ? "供给分配已完成"
                        : "本次供给分配已保存",
                    description: !taskCompleted
                        ? "当前责任范围仍有未分配数量，任务继续保留在工作台，请完成剩余供给分配。"
                        : count === 0
                          ? `已从现有库存建立 ${stockCount} 条销售预留并生成仓发草稿，无需采购。`
                          : stockCount > 0
                            ? `已建立 ${stockCount} 条库存预留，并将缺口拆成 ${count} 张采购单提交审批。`
                            : count > 1
                              ? `已将缺口拆成 ${count} 张采购单并提交审批。`
                              : "已创建 1 张采购单并提交审批。",
                    type: "success",
                    timeout: 4000,
                })
                if (embedded && order.workItemId && taskCompleted) {
                    onTaskCompleted?.(order.workItemId)
                } else if (taskCompleted) {
                    router.replace("/procurement/orders")
                } else {
                    await basesQuery.refetch()
                }
                return
            }
            if (result.status === "failed") {
                createIntentRef.current = null
                if (result.code === "CONFLICT") {
                    await basesQuery.refetch()
                }
                setActionError({
                    title: "供给分配失败",
                    description:
                        result.code === "CONFLICT"
                            ? `${result.message} 创建依据已刷新，请核对后重试。`
                            : result.message,
                })
                return
            }
            setActionError({
                title: "供给分配结果待确认",
                description: `${result.message} 请保留当前页面并使用同一操作重试，系统已记录本次提交，刷新后查看结果。`,
            })
        },
    })

    React.useEffect(() => {
        if (initialSalesOrderId) return
        const current = form.state.values.salesOrderId
        if (
            current &&
            workspace.some((order) => order.salesOrderId === current)
        ) {
            return
        }
        const first = workspace[0]?.salesOrderId ?? ""
        if (first) form.setFieldValue("salesOrderId", first)
    }, [form, initialSalesOrderId, workspace])

    const selectedSalesOrderId = useStore(
        form.store,
        (state) => state.values.salesOrderId,
    )
    const selectedOrder = workspace.find(
        (order) => order.salesOrderId === selectedSalesOrderId,
    )
    const writeLines = React.useCallback(
        (next: SourcingLineInput[]) => {
            form.setFieldValue("lines", next)
            next.forEach((line, index) => {
                form.setFieldValue(`lines[${index}].rowKey`, line.rowKey)
                form.setFieldValue(
                    `lines[${index}].salesOrderLineId`,
                    line.salesOrderLineId,
                )
                form.setFieldValue(`lines[${index}].selected`, line.selected)
                form.setFieldValue(`lines[${index}].basisId`, line.basisId)
                form.setFieldValue(
                    `lines[${index}].targetWarehouseId`,
                    line.targetWarehouseId,
                )
                form.setFieldValue(
                    `lines[${index}].targetWarehouseName`,
                    line.targetWarehouseName,
                )
                form.setFieldValue(`lines[${index}].quantity`, line.quantity)
                form.setFieldValue(
                    `lines[${index}].expectedDeliveryDate`,
                    line.expectedDeliveryDate,
                )
            })
        },
        [form],
    )

    React.useEffect(() => {
        // URL 已带 salesOrderId 时选中项首屏就是最终值，必须等创建依据到达后再写默认明细。
        if (basesQuery.isPending) return
        writeLines(buildDefaultSourcingLines(selectedOrder))
        setPreviewOpen(false)
    }, [basesQuery.isPending, selectedOrder, writeLines])

    const lines = useStore(form.store, (state) => state.values.lines)
    const previews = React.useMemo(
        () => buildPurchaseOrderPreviews(selectedOrder, lines),
        [lines, selectedOrder],
    )
    const stockPreviews = React.useMemo(
        () => buildStockAllocationPreviews(selectedOrder, lines),
        [lines, selectedOrder],
    )
    const previewTotals = React.useMemo(
        () =>
            sumPreviewTotals(previews.flatMap((preview) => [...preview.lines])),
        [previews],
    )
    const confirmationDescription =
        stockPreviews.length > 0 && previews.length > 0
            ? `将建立 ${stockPreviews.length} 条库存预留，并为剩余缺口创建 ${previews.length} 张采购单提交审批，采购含税合计 ${previewTotals.gross}。`
            : stockPreviews.length > 0
              ? `将建立 ${stockPreviews.length} 条库存预留；现有库存已满足本次分配，无需创建采购单。`
              : `将为供给缺口创建 ${previews.length} 张采购单提交审批，采购含税合计 ${previewTotals.gross}。`
    const commonSourcingOptions = React.useMemo(
        () => commonSourcingOptionsForSelected(selectedOrder, lines),
        [lines, selectedOrder],
    )
    const selectedCount = lines.filter((line) => line.selected).length

    const applyBatchSupplier = React.useCallback(
        (basisId: string) => {
            if (!selectedOrder) return
            const selected = lines.filter((line) => line.selected)
            const targets = selected.length > 0 ? selected : lines
            let applied = 0
            const targetKeys = new Set(targets.map((line) => line.rowKey))
            lines.forEach((line, index) => {
                if (targets.length > 0 && !targetKeys.has(line.rowKey)) {
                    return
                }
                const product = selectedOrder.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, basisId)
                if (!option) return
                form.setFieldValue(`lines[${index}].selected`, true)
                form.setFieldValue(`lines[${index}].basisId`, basisId)
                if (
                    option.sourceType !== "PURCHASE" ||
                    option.fulfillmentResponsibility !== "WAREHOUSE"
                ) {
                    form.setFieldValue(`lines[${index}].targetWarehouseId`, "")
                    form.setFieldValue(
                        `lines[${index}].targetWarehouseName`,
                        "",
                    )
                }
                form.setFieldValue(
                    `lines[${index}].quantity`,
                    option.maxCreateQuantity,
                )
                form.setFieldValue(
                    `lines[${index}].expectedDeliveryDate`,
                    option.expectedDeliveryDate,
                )
                applied += 1
            })
            toast.add({
                title: applied > 0 ? "已批量指定履约方案" : "没有可应用的明细",
                description:
                    applied > 0
                        ? `已为 ${applied} 条明细填入该履约方案。`
                        : "选中行不支持该履约方案。",
                type: applied > 0 ? "success" : "warning",
                timeout: 4000,
            })
        },
        [form, lines, selectedOrder],
    )

    const applyBestSourcingOptions = React.useCallback(() => {
        if (!selectedOrder) return
        const base = buildDefaultSourcingLines(selectedOrder)
        const next = assignBestSourcingOptions(selectedOrder, base)
        writeLines(next)
        const filled = next.filter((line) => line.basisId).length
        toast.add({
            title: filled > 0 ? "已重新分配供给" : "没有可匹配的供给方案",
            description:
                filled > 0
                    ? "已优先分配现有库存，并为剩余缺口推荐采购方案。"
                    : "当前明细没有可用库存或合格采购供给。",
            type: filled > 0 ? "success" : "warning",
            timeout: 4000,
        })
    }, [selectedOrder, writeLines])

    const addSplitLine = React.useCallback(
        (salesOrderLineId: string) => {
            if (!selectedOrder) return
            const product = selectedOrder.lines.find(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (!product) return
            const used = new Set(
                lines
                    .filter(
                        (line) =>
                            line.salesOrderLineId === salesOrderLineId &&
                            line.basisId,
                    )
                    .map((line) => line.basisId),
            )
            const option = product.options.find(
                (candidate) => !used.has(candidate.basisId),
            )
            if (!option) {
                toast.add({
                    title: "没有更多履约方案",
                    description: "该销售明细的可选履约方案都已添加。",
                    type: "warning",
                    timeout: 4000,
                })
                return
            }
            const lastIndex = lines.findLastIndex(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            const next = [...lines]
            next.splice(lastIndex + 1, 0, {
                rowKey: `${salesOrderLineId}:${crypto.randomUUID()}`,
                salesOrderLineId,
                selected: true,
                quantity: "",
                basisId: option.basisId,
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: option.expectedDeliveryDate,
            })
            writeLines(next)
        },
        [lines, selectedOrder, writeLines],
    )

    const removeSplitLine = React.useCallback(
        (rowKey: string) => {
            const target = lines.find((line) => line.rowKey === rowKey)
            if (!target) return
            const siblings = lines.filter(
                (line) => line.salesOrderLineId === target.salesOrderLineId,
            )
            if (siblings.length <= 1) return
            writeLines(lines.filter((line) => line.rowKey !== rowKey))
        },
        [lines, writeLines],
    )

    const canMatchBest = Boolean(
        selectedOrder?.lines.some((line) => line.options.length > 0),
    )

    const openPreview = React.useCallback(async () => {
        for (const field of Object.values(form.fieldInfo)) {
            const instance = field?.instance
            if (!instance || instance.store.state.meta.isTouched) continue
            instance.setMeta((prev) => ({ ...prev, isTouched: true }))
        }
        await form.validate("submit")
        if (form.state.canSubmit) {
            setActionError(null)
            setPreviewOpen(true)
            return
        }
        const description =
            collectSourcingErrorMessages(form.getAllErrors()).join("；") ||
            "请检查本次分配数量和供给方案后重试。"
        setActionError({
            title: "无法预览供给分配",
            description,
        })
        toast.add({
            title: "无法预览供给分配",
            description,
            type: "error",
            timeout: 6000,
        })
    }, [form])

    if (basesQuery.isPending) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                {embedded ? null : (
                    <PageHeader
                        title="供给分配"
                        description="正在加载库存与采购供给…"
                    />
                )}
                <div
                    className="flex flex-col gap-3"
                    aria-busy="true"
                    aria-label="加载中"
                >
                    <Skeleton className="h-16" />
                    <Skeleton className="h-40" />
                </div>
            </PageScaffold>
        )
    }

    if (basesQuery.isError) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                {embedded ? null : (
                    <PageHeader
                        title="供给分配"
                        description="供给依据加载失败"
                    />
                )}
                <BusinessFailureState
                    error={basesQuery.error}
                    action={
                        <Button
                            id="procurement-orders-create-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void basesQuery.refetch()}
                        >
                            重新加载
                        </Button>
                    }
                    details={
                        !embedded ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={
                                    <Link
                                        id="procurement-orders-create-back"
                                        href="/procurement/orders"
                                    />
                                }
                            >
                                返回列表
                            </Button>
                        ) : undefined
                    }
                />
            </PageScaffold>
        )
    }

    const sourcingTotalItems = [
        {
            id: "orders",
            label: "将创建采购单",
            value: `${previews.length} 张`,
        },
        {
            id: "stock",
            label: "将建立库存预留",
            value: `${stockPreviews.length} 条`,
        },
        {
            id: "lines",
            label: "采购缺口明细",
            value: `${previews.reduce((count, preview) => count + preview.lines.length, 0)} 行`,
        },
        {
            id: "gross",
            label: "采购含税合计",
            value: <MoneyValue value={previewTotals.gross} />,
        },
    ]

    const previewAction = (
        <Button
            id="procurement-orders-create-preview"
            type="button"
            data-testid="purchase-create-preview"
            onClick={() => void openPreview()}
        >
            预览供给分配
        </Button>
    )

    return (
        <PageScaffold
            density={embedded ? "compact" : "default"}
            className={embedded ? workspaceEmbeddedScaffoldClassName : "pb-8"}
        >
            {embedded ? null : (
                <PageHeader
                    title="供给分配"
                    description="系统优先推荐现有库存，不足部分再推荐采购；确认后一次完成库存预留和采购缺口建单。"
                    actions={
                        <PageActions
                            actions={[
                                {
                                    actionKey: "back",
                                    label: "返回列表",
                                    icon: ArrowLeftIcon,
                                    variant: "outline",
                                    onClick: () =>
                                        router.push("/procurement/orders"),
                                    id: "procurement-orders-create-back-header",
                                },
                            ]}
                        />
                    }
                />
            )}

            <div
                className={
                    embedded ? "min-h-0 flex-1 overflow-auto" : undefined
                }
            >
                {actionError ? (
                    <Alert
                        variant="destructive"
                        className={embedded ? "mx-5 my-5" : undefined}
                    >
                        <AlertTitle>{actionError.title}</AlertTitle>
                        <AlertDescription>
                            {actionError.description}
                        </AlertDescription>
                    </Alert>
                ) : null}

                {workspace.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-data"
                        className={
                            embedded
                                ? workspaceTaskSurfacePadClassName
                                : undefined
                        }
                        title="当前没有待分配供给"
                        description={
                            initialSalesOrderId
                                ? "该销售单可能尚未生效、供给已覆盖，或既无可用库存也无合格采购供给。"
                                : "当前没有待分配供给。请检查已生效销售单、库存余额和供应商供给。"
                        }
                        action={
                            !embedded ? (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    render={
                                        <Link
                                            id="procurement-orders-create-empty-back"
                                            href="/procurement/orders"
                                        />
                                    }
                                >
                                    返回列表
                                </Button>
                            ) : undefined
                        }
                    />
                ) : (
                    <form
                        className={cn(
                            "flex flex-col",
                            embedded ? "gap-0" : "gap-4",
                        )}
                        onSubmit={(event) => {
                            event.preventDefault()
                            event.stopPropagation()
                        }}
                    >
                        <PurchaseOrderCreateSourcePanel
                            workspace={workspace}
                            selectedSalesOrderId={selectedSalesOrderId}
                            selectedOrder={selectedOrder}
                            disabled={Boolean(initialSalesOrderId)}
                            flat={embedded}
                            salesOrderReturnTo={salesOrderReturnTo}
                            onPreviewSalesOrder={
                                renderSalesOrderPreview
                                    ? (id, title) =>
                                          setSourcePaper({ id, title })
                                    : undefined
                            }
                            onSalesOrderChange={(value) =>
                                form.setFieldValue("salesOrderId", value)
                            }
                        />

                        {selectedOrder ? (
                            <section
                                className={cn(
                                    "overflow-hidden",
                                    surfaceClassName(embedded),
                                    embedded &&
                                        cn(
                                            workspaceTaskSurfacePadClassName,
                                            "py-5",
                                        ),
                                )}
                            >
                                <div
                                    className={cn(
                                        "flex flex-col gap-3",
                                        !embedded && "p-4 md:p-5",
                                    )}
                                >
                                    <div className="flex items-center justify-between gap-2">
                                        <h2 className="font-heading text-sm font-semibold">
                                            销售明细与供给方案
                                        </h2>
                                        <span className="text-xs text-muted-foreground">
                                            {selectedOrder.lines.length} 行
                                        </span>
                                    </div>
                                    <PurchaseOrderCreateBatchBar
                                        selectedCount={selectedCount}
                                        options={commonSourcingOptions}
                                        onApply={applyBatchSupplier}
                                        matchDisabled={!canMatchBest}
                                        onMatchBest={applyBestSourcingOptions}
                                    />
                                    {sourcingFormLinesReady(
                                        lines,
                                        selectedOrder,
                                    ) ? (
                                        <PurchaseOrderCreateSourcingTable
                                            form={
                                                form as unknown as PurchaseOrderCreateFormApi
                                            }
                                            order={selectedOrder}
                                            onAddSplit={addSplitLine}
                                            onRemoveSplit={removeSplitLine}
                                        />
                                    ) : (
                                        <Skeleton className="h-40" />
                                    )}
                                    <p className="text-xs text-muted-foreground">
                                        页面已自动优先分配现有库存；库存不足时，再按可覆盖数量、成本和交期推荐采购。可调整或拆分，同一采购维度会合并为一张采购单。
                                    </p>
                                </div>
                            </section>
                        ) : null}

                        {embedded ? null : (
                            <StickyTotalBar
                                items={sourcingTotalItems}
                                actions={previewAction}
                            />
                        )}
                    </form>
                )}
            </div>
            {embedded && workspace.length > 0 ? (
                <WorkspaceTaskFooter
                    fallback={
                        <div
                            className={cn(
                                workspaceTaskSurfacePadClassName,
                                "flex shrink-0 flex-col items-stretch gap-3 border-t border-border/40 py-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4",
                            )}
                        >
                            <PurchaseOrderCreateFooterTotals
                                items={sourcingTotalItems}
                                action={previewAction}
                            />
                        </div>
                    }
                >
                    <PurchaseOrderCreateFooterTotals
                        items={sourcingTotalItems}
                        action={previewAction}
                    />
                </WorkspaceTaskFooter>
            ) : null}

            {renderSalesOrderPreview?.({
                salesOrderId: sourcePaper?.id ?? null,
                title: sourcePaper?.title,
                open: Boolean(sourcePaper),
                onOpenChange: (open) => {
                    if (!open) setSourcePaper(null)
                },
            })}

            <PurchaseOrderCreatePreviewDialog
                open={previewOpen}
                previews={previews}
                stockAllocations={stockPreviews}
                sourceOrder={selectedOrder}
                creating={createMutation.isPending}
                actionError={actionError}
                onOpenChange={setPreviewOpen}
                onConfirm={() => {
                    setPreviewOpen(false)
                    setConfirmOpen(true)
                }}
            />

            <AlertDialog
                open={confirmOpen}
                onOpenChange={(open) => {
                    setConfirmOpen(open)
                    if (open) {
                        submittingFromConfirmRef.current = false
                        return
                    }
                    if (!submittingFromConfirmRef.current) {
                        setPreviewOpen(true)
                    }
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>确认供给分配</AlertDialogTitle>
                        <AlertDialogDescription>
                            {confirmationDescription}
                            库存和采购会在同一次提交中生效。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            id="procurement-orders-create-confirm-cancel"
                            disabled={createMutation.isPending}
                        >
                            返回预览
                        </AlertDialogCancel>
                        <AlertDialogAction
                            id="procurement-orders-create-confirm"
                            data-testid="purchase-create-confirm"
                            disabled={createMutation.isPending}
                            onClick={() => {
                                submittingFromConfirmRef.current = true
                                setConfirmOpen(false)
                                void form.handleSubmit()
                            }}
                        >
                            {createMutation.isPending ? "提交中…" : "确认提交"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </PageScaffold>
    )
}

function PurchaseOrderCreateFooterTotals({
    items,
    action,
}: {
    items: readonly {
        id: string
        label: string
        value: ReactNode
    }[]
    action: ReactNode
}) {
    return (
        <div className="flex w-full min-w-0 flex-col items-stretch gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
            <dl className="grid min-w-0 flex-1 grid-cols-2 gap-x-6 gap-y-2 sm:grid-cols-4">
                {items.map((item) => (
                    <div key={item.id} className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            {item.label}
                        </dt>
                        <dd className="num mt-0.5 font-medium">{item.value}</dd>
                    </div>
                ))}
            </dl>
            {action}
        </div>
    )
}
