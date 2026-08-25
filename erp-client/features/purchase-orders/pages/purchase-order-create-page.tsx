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
    WizardSteps,
    surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
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
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Field, FieldLabel } from "@/components/ui/field"
import { PurchaseOrderCreateBatchBar } from "@/features/purchase-orders/components/purchase-order-create-batch-bar"
import { PurchaseOrderCreatePreview } from "@/features/purchase-orders/components/purchase-order-create-preview"
import { PurchaseOrderCreateSourcingTable } from "@/features/purchase-orders/components/purchase-order-create-sourcing-table"
import {
    useCreateFromSourcingMutation,
    useCreationBasesQuery,
} from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderCreateFormApi } from "@/features/purchase-orders/lib/purchase-order-create-form-types"
import {
    buildDefaultSourcingLines,
    buildPurchaseOrderPreviews,
    buildSourcingWorkspace,
    commonSuppliersForSelected,
    findSourcingOption,
    sumPreviewTotals,
    type SourcingLineInput,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import { buildSourcingFormSchema } from "@/features/purchase-orders/lib/purchase-order-create-validation"
import type { CreatedPurchaseOrderDraft } from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"

const CREATE_STEPS = [
    { id: "sourcing", label: "选择供应商" },
    { id: "preview", label: "预览采购单" },
] as const

type CreateStep = (typeof CREATE_STEPS)[number]["id"]

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
    const [step, setStep] = React.useState<CreateStep>("sourcing")
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
                const parsed = buildSourcingFormSchema(order).safeParse(value)
                if (parsed.success) return undefined
                return parsed.error
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
    React.useEffect(() => {
        const order = workspaceRef.current.find(
            (candidate) => candidate.salesOrderId === selectedSalesOrderId,
        )
        form.setFieldValue("lines", buildDefaultSourcingLines(order))
        setStep("sourcing")
        setCreatedOrders(null)
    }, [form, selectedSalesOrderId])

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
            lines.forEach((line, index) => {
                if (!line.selected) return
                const product = selectedOrder.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, supplierId)
                if (!option) return
                form.setFieldValue(`lines[${index}].supplierId`, supplierId)
                form.setFieldValue(
                    `lines[${index}].quantity`,
                    option.maxCreateQuantity,
                )
            })
        },
        [form, lines, selectedOrder],
    )

    const openPreview = React.useCallback(async () => {
        await form.validate("submit")
        if (!form.state.canSubmit) return
        setActionError(null)
        setStep("preview")
    }, [form])

    if (basesQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title="新建采购单"
                    description="正在加载可采购明细…"
                />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
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
                metadata={
                    <WizardSteps steps={CREATE_STEPS} currentStepId={step} />
                }
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
                    <section
                        className={cn(surfacePanelClassName, "overflow-hidden")}
                    >
                        <div className="space-y-4 p-4 md:p-5">
                            <Field>
                                <FieldLabel>来源销售单</FieldLabel>
                                <OptionCombobox
                                    className="w-full max-w-xl"
                                    value={selectedSalesOrderId || null}
                                    allowClear={false}
                                    disabled={Boolean(initialSalesOrderId)}
                                    aria-label="选择来源销售单"
                                    placeholder="选择来源销售单"
                                    options={workspace.map((order) => ({
                                        value: order.salesOrderId,
                                        label: `${order.salesOrderNo} · ${order.customerName}`,
                                        keywords: `${order.salesOrderId} ${order.salesOwnerName ?? ""}`,
                                    }))}
                                    onValueChange={(value) =>
                                        form.setFieldValue(
                                            "salesOrderId",
                                            value ?? "",
                                        )
                                    }
                                />
                            </Field>
                            {selectedOrder ? (
                                <DescriptionList columns="three">
                                    <DescriptionItem>
                                        <DescriptionTerm>客户</DescriptionTerm>
                                        <DescriptionDetails>
                                            {selectedOrder.customerName}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>合同</DescriptionTerm>
                                        <DescriptionDetails className="num">
                                            {selectedOrder.contractNumber ??
                                                "无合同"}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            负责销售
                                        </DescriptionTerm>
                                        <DescriptionDetails>
                                            {selectedOrder.salesOwnerName ??
                                                "—"}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                </DescriptionList>
                            ) : null}
                        </div>
                    </section>

                    {step === "sourcing" && selectedOrder ? (
                        <section
                            className={cn(
                                surfacePanelClassName,
                                "overflow-hidden",
                            )}
                        >
                            <div className="space-y-3 p-4 md:p-5">
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
                                />
                                <PurchaseOrderCreateSourcingTable
                                    form={
                                        form as unknown as PurchaseOrderCreateFormApi
                                    }
                                    order={selectedOrder}
                                />
                                <p className="text-xs text-muted-foreground">
                                    可逐行选择供应商，或勾选多行后批量指定共同可选供应商。同一供应商、采购类型、付款条件和履约责任的明细会合并为一张采购单。
                                </p>
                            </div>
                        </section>
                    ) : null}

                    {step === "preview" ? (
                        <PurchaseOrderCreatePreview previews={previews} />
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
                        leftActions={
                            step === "preview" ? (
                                <Button
                                    type="button"
                                    variant="outline"
                                    onClick={() => setStep("sourcing")}
                                >
                                    返回选源
                                </Button>
                            ) : undefined
                        }
                        actions={
                            step === "sourcing" ? (
                                <Button
                                    type="button"
                                    data-testid="purchase-create-preview"
                                    onClick={() => void openPreview()}
                                >
                                    预览采购单
                                </Button>
                            ) : (
                                <Button
                                    type="button"
                                    data-testid="purchase-create-from-basis"
                                    disabled={
                                        previews.length === 0 ||
                                        createMutation.isPending
                                    }
                                    onClick={() => setConfirmOpen(true)}
                                >
                                    {createMutation.isPending
                                        ? "创建中…"
                                        : `确认创建 ${previews.length} 张采购草稿`}
                                </Button>
                            )
                        }
                    />
                </form>
            )}

            <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
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
