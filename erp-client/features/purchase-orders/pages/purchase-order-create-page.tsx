"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useStore } from "@tanstack/react-form"
import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
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
    assignBestSuppliers,
    buildDefaultSourcingLines,
    buildPurchaseOrderPreviews,
    buildSourcingWorkspace,
    commonSuppliersForSelected,
    findSourcingOption,
    sumPreviewTotals,
    type SourcingLineInput,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    collectSourcingErrorMessages,
    sourcingFormValidationError,
} from "@/features/purchase-orders/lib/purchase-order-create-validation"
import type { CreatedPurchaseOrderDraft } from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"

/**
 * 新建采购单页面：按销售明细选源，预览拆单结果后确认创建真实草稿。
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
    const [createdOrders, setCreatedOrders] = React.useState<
        CreatedPurchaseOrderDraft[] | null
    >(null)
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
                (line) => line.selected && line.supplierId,
            )
            const fingerprint = JSON.stringify({
                salesOrderId: order.salesOrderId,
                workItemId: order.workItemId,
                lines: selected.map((line) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    supplierId: line.supplierId,
                    quantity: line.quantity.trim(),
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
                    supplierId: line.supplierId,
                    quantity: line.quantity.trim(),
                })),
                idempotencyKey: createIntentRef.current.idempotencyKey,
            })
            if (result.status === "succeeded") {
                createIntentRef.current = null
                const orders = result.data.orders
                if (orders.length === 1 && orders[0]) {
                    router.push(
                        `/procurement/orders/${orders[0].purchaseOrderId}?mode=edit`,
                    )
                    return
                }
                setCreatedOrders(orders)
                setActionError(null)
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
                form.setFieldValue(
                    `lines[${index}].salesOrderLineId`,
                    line.salesOrderLineId,
                )
                form.setFieldValue(`lines[${index}].selected`, line.selected)
                form.setFieldValue(
                    `lines[${index}].supplierId`,
                    line.supplierId,
                )
                form.setFieldValue(`lines[${index}].quantity`, line.quantity)
            })
        },
        [form],
    )

    React.useEffect(() => {
        const order = workspaceRef.current.find(
            (candidate) => candidate.salesOrderId === selectedSalesOrderId,
        )
        writeLines(buildDefaultSourcingLines(order))
        setPreviewOpen(false)
        setCreatedOrders(null)
    }, [selectedSalesOrderId, writeLines])

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
    const commonSuppliers = React.useMemo(
        () => commonSuppliersForSelected(selectedOrder, lines),
        [lines, selectedOrder],
    )
    const selectedCount = lines.filter((line) => line.selected).length

    const applyBatchSupplier = React.useCallback(
        (supplierId: string) => {
            if (!selectedOrder) return
            const selected = lines.filter((line) => line.selected)
            const targets = selected.length > 0 ? selected : lines
            let applied = 0
            lines.forEach((line, index) => {
                if (
                    targets.length > 0 &&
                    !targets.some(
                        (candidate) =>
                            candidate.salesOrderLineId ===
                            line.salesOrderLineId,
                    )
                ) {
                    return
                }
                const product = selectedOrder.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, supplierId)
                if (!option) return
                form.setFieldValue(`lines[${index}].selected`, true)
                form.setFieldValue(`lines[${index}].supplierId`, supplierId)
                form.setFieldValue(
                    `lines[${index}].quantity`,
                    option.maxCreateQuantity,
                )
                applied += 1
            })
            toast.add({
                title: applied > 0 ? "已批量指定供应商" : "没有可应用的明细",
                description:
                    applied > 0
                        ? `已为 ${applied} 条明细填入该供应商。`
                        : "选中行没有这家供应商的合格供给。",
                type: applied > 0 ? "success" : "warning",
                timeout: 4000,
            })
        },
        [form, lines, selectedOrder],
    )

    const applyBestSuppliers = React.useCallback(() => {
        if (!selectedOrder) return
        const base =
            lines.length === selectedOrder.lines.length
                ? lines
                : buildDefaultSourcingLines(selectedOrder)
        const next = assignBestSuppliers(selectedOrder, base)
        writeLines(next)
        const filled = next.filter((line) => line.supplierId).length
        toast.add({
            title: filled > 0 ? "已匹配最优供应商" : "没有可匹配的供应商",
            description:
                filled > 0
                    ? `已为 ${filled} 条明细填入最低含税成本、可覆盖剩余数量的供应商。`
                    : "当前明细没有合格供给。",
            type: filled > 0 ? "success" : "warning",
            timeout: 4000,
        })
    }, [lines, selectedOrder, writeLines])

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
            "请检查本次采购数量和供应商后重试。"
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

    if (createdOrders) {
        return (
            <PageScaffold>
                <PageHeader
                    title="采购草稿已创建"
                    description={`已按供应商拆成 ${createdOrders.length} 张采购单。`}
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
                <FormalActionResult
                    status="succeeded"
                    title="已创建采购草稿"
                    description="可分别打开各张草稿继续编辑或提交审批。"
                    actions={
                        <div className="flex flex-wrap gap-2">
                            {createdOrders.map((order) => (
                                <Button
                                    key={order.purchaseOrderId}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    render={
                                        <Link
                                            href={`/procurement/orders/${order.purchaseOrderId}?mode=edit`}
                                        />
                                    }
                                >
                                    {order.draftLabel}
                                </Button>
                            ))}
                        </div>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold className="pb-8">
            <PageHeader
                title="新建采购单"
                description="先为每条销售明细选择供应商，再预览将要创建的采购单。"
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
                                        销售明细与供应商
                                    </h2>
                                    <span className="text-xs text-muted-foreground">
                                        {selectedOrder.lines.length} 行
                                    </span>
                                </div>
                                <PurchaseOrderCreateBatchBar
                                    selectedCount={selectedCount}
                                    options={commonSuppliers}
                                    onApply={applyBatchSupplier}
                                    matchDisabled={!canMatchBest}
                                    onMatchBest={applyBestSuppliers}
                                />
                                {lines.length === selectedOrder.lines.length &&
                                lines.every(
                                    (line, index) =>
                                        line.salesOrderLineId ===
                                        selectedOrder.lines[index]
                                            ?.salesOrderLineId,
                                ) ? (
                                    <PurchaseOrderCreateSourcingTable
                                        form={
                                            form as unknown as PurchaseOrderCreateFormApi
                                        }
                                        order={selectedOrder}
                                    />
                                ) : (
                                    <Skeleton className="h-40" />
                                )}
                                <p className="text-xs text-muted-foreground">
                                    可逐行选择供应商，一键按最低含税成本和最早交期匹配最优供应商，或勾选多行后批量指定共同可选供应商。同一供应商、采购类型、付款条件和履约责任的明细会合并为一张采购单。
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
                        <AlertDialogTitle>确认创建采购单</AlertDialogTitle>
                        <AlertDialogDescription>
                            将按当前选源结果创建 {previews.length}{" "}
                            张采购草稿，含税合计 {previewTotals.gross}
                            。创建后可分别打开草稿编辑或提交审批。
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
                            确认创建
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </PageScaffold>
    )
}
