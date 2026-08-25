"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
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
    surfacePanelClassName,
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
export function PurchaseOrderCreatePage({
    initialSalesOrderId = "",
    initialWorkItemId = "",
}: {
    initialSalesOrderId?: string
    initialWorkItemId?: string
}) {
    const router = useRouter()
    const basesQuery = useCreationBasesQuery({
        salesOrderId: initialSalesOrderId || undefined,
        workItemId: initialWorkItemId || undefined,
    })
    const createMutation = useCreateFromSourcingMutation()
    const [previewOpen, setPreviewOpen] = React.useState(false)
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
            const selected = value.lines.filter(
                (line) => line.selected && line.basisId,
            )
            const fingerprint = JSON.stringify({
                salesOrderId: order.salesOrderId,
                workItemId: order.workItemId,
                lines: selected.map((line) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    basisId: line.basisId,
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
                lines: selected.map((line) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    basisId: line.basisId,
                    quantity: line.quantity.trim(),
                    expectedDeliveryDate: line.expectedDeliveryDate,
                })),
                idempotencyKey: createIntentRef.current.idempotencyKey,
            })
            if (result.status === "succeeded") {
                createIntentRef.current = null
                const count = result.data.orders.length
                toast.add({
                    title: "采购单已提交审批",
                    description:
                        count > 1
                            ? `已按履约方案拆成 ${count} 张采购单并提交审批。`
                            : "已创建 1 张采购单并提交审批。",
                    type: "success",
                    timeout: 4000,
                })
                router.replace("/procurement/orders")
                return
            }
            if (result.status === "failed") {
                createIntentRef.current = null
                if (result.code === "CONFLICT") {
                    await basesQuery.refetch()
                }
                setActionError({
                    title: "建单失败",
                    description:
                        result.code === "CONFLICT"
                            ? `${result.message} 创建依据已刷新，请核对后重试。`
                            : result.message,
                })
                return
            }
            setActionError({
                title: "建单结果待确认",
                description: `${result.message} 请保留当前页面并使用同一操作重试，系统会复用本次幂等键。`,
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
    const previewTotals = React.useMemo(
        () =>
            sumPreviewTotals(previews.flatMap((preview) => [...preview.lines])),
        [previews],
    )
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
            title: filled > 0 ? "已匹配最优履约方案" : "没有可匹配的履约方案",
            description:
                filled > 0
                    ? `已为 ${filled} 条明细填入最低含税成本、可覆盖剩余数量的履约方案。`
                    : "当前明细没有合格履约方案。",
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
            "请检查本次采购数量和履约方案后重试。"
        setActionError({
            title: "无法预览采购单",
            description,
        })
        toast.add({
            title: "无法预览采购单",
            description,
            type: "error",
            timeout: 6000,
        })
    }, [form])

    if (basesQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title="新建采购单"
                    description="正在加载可采购明细…"
                />
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
            <PageScaffold>
                <PageHeader title="新建采购单" description="创建依据加载失败" />
                <BusinessFailureState
                    error={basesQuery.error}
                    onRetry={() => void basesQuery.refetch()}
                    retryLabel="重新加载"
                    details={
                        <Button
                            type="button"
                            variant="outline"
                            render={<Link href="/procurement/orders" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold className="pb-8">
            <PageHeader
                title="新建采购单"
                description="先为每条销售明细选择履约方案，可按数量拆分，再预览将要创建并提交审批的采购单。"
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
                            },
                        ]}
                    />
                }
            />

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>{actionError.title}</AlertTitle>
                    <AlertDescription>
                        {actionError.description}
                    </AlertDescription>
                </Alert>
            ) : null}

            {workspace.length === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="当前没有可建采购依据"
                    description={
                        initialSalesOrderId
                            ? "该销售单当前没有可建采购依据。可能尚未生效、没有合格供给，或待采购数量已覆盖。"
                            : "当前没有可建采购依据。请从已生效销售单的履约页进入，或检查商品是否存在合格供给。"
                    }
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            render={<Link href="/procurement/orders" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            ) : (
                <form
                    className="flex flex-col gap-4"
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
                        onSalesOrderChange={(value) =>
                            form.setFieldValue("salesOrderId", value)
                        }
                    />

                    {selectedOrder ? (
                        <section
                            className={cn(
                                surfacePanelClassName,
                                "overflow-hidden",
                            )}
                        >
                            <div className="flex flex-col gap-3 p-4 md:p-5">
                                <div className="flex items-center justify-between gap-2">
                                    <h2 className="font-heading text-sm font-semibold">
                                        销售明细与履约方案
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
                                    可逐行选择供应商与履约责任，也可拆分同一销售明细的数量。一键匹配会选择含税成本最低且可覆盖剩余数量的方案；同一供应商、采购类型、付款条件和履约责任的明细会合并为一张采购单。
                                </p>
                            </div>
                        </section>
                    ) : null}

                    <StickyTotalBar
                        items={[
                            {
                                id: "orders",
                                label: "将创建采购单",
                                value: `${previews.length} 张`,
                            },
                            {
                                id: "lines",
                                label: "本次采购明细",
                                value: `${previews.reduce((count, preview) => count + preview.lines.length, 0)} 行`,
                            },
                            {
                                id: "gross",
                                label: "含税合计",
                                value: (
                                    <MoneyValue value={previewTotals.gross} />
                                ),
                            },
                        ]}
                        actions={
                            <Button
                                type="button"
                                data-testid="purchase-create-preview"
                                onClick={() => void openPreview()}
                            >
                                预览采购单
                            </Button>
                        }
                    />
                </form>
            )}

            <PurchaseOrderCreatePreviewDialog
                open={previewOpen}
                previews={previews}
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
                        <AlertDialogTitle>确认创建并提交审批</AlertDialogTitle>
                        <AlertDialogDescription>
                            将按当前选源结果创建 {previews.length}{" "}
                            张采购单并提交审批，含税合计 {previewTotals.gross}
                            。提交成功后将回到采购单列表。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel>返回预览</AlertDialogCancel>
                        <AlertDialogAction
                            data-testid="purchase-create-confirm"
                            onClick={() => {
                                submittingFromConfirmRef.current = true
                                setConfirmOpen(false)
                                void form.handleSubmit()
                            }}
                        >
                            确认提交
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </PageScaffold>
    )
}
