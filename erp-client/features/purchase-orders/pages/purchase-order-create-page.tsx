"use client"

import * as React from "react"
import type { ReactNode } from "react"
import Link from "next/link"
import { useStore } from "@tanstack/react-form"
import { ArrowLeftIcon, TriangleAlertIcon } from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    MoneyValue,
    PageActions,
    PageScaffold,
    StickyTotalBar,
    WorkspaceTaskFooter,
    workspaceEmbeddedScaffoldClassName,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { SystemFeedback } from "@/components/business/system-feedback"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SourcingOptionsRefreshContext } from "@/features/purchase-orders/components/create-sourcing/options-refresh-context"
import {
    PurchaseOrderCreateSelectionBar,
    PurchaseOrderCreateUtilityActions,
} from "@/features/purchase-orders/components/purchase-order-create-batch-bar"
import { PurchaseOrderCreatePreviewDialog } from "@/features/purchase-orders/components/purchase-order-create-preview"
import { PurchaseOrderCreateSourcePanel } from "@/features/purchase-orders/components/purchase-order-create-source-panel"
import {
    PurchaseOrderCreateSourcingWorkspace,
    type SourcingFocusRequest,
} from "@/features/purchase-orders/components/purchase-order-create-sourcing-workspace"
import {
    useCreateFromSourcingMutation,
    useCreationBasesQuery,
} from "@/features/purchase-orders/hooks/queries"
import type {
    PurchaseOrderCreateFormApi,
    PurchaseOrderCreateFormValues,
} from "@/features/purchase-orders/lib/purchase-order-create-form-types"
import {
    assignBestSourcingOptions,
    buildDefaultSourcingLines,
    buildPurchaseOrderPreviews,
    buildSourcingWorkspace,
    buildStockAllocationPreviews,
    findSourcingOption,
    pickBestSourcingOption,
    sourcingFormLinesReady,
    sumPreviewTotals,
    type SourcingLineInput,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    collectSourcingErrorMessages,
    sourcingFormValidationError,
} from "@/features/purchase-orders/lib/purchase-order-create-validation"
import { getErrorMessage } from "@/lib/api/errors"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import {
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "@/lib/inquiry-local"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import {
    sourcingDraftKey,
    useSourcingCommand,
    type SavedSourcingCommand,
} from "../hooks/use-sourcing-command"
import { useSourcingParticipation } from "../hooks/use-sourcing-participation"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import { buildFillDirectShortfallPatches } from "../lib/sourcing/address-grouping"
import { sourcingDeliveryAllocations } from "../lib/sourcing/delivery"
import {
    availableDeliveryRows,
    deliveryQuantity,
    usesAddressQuantity,
} from "../lib/sourcing/delivery-editor"
import { addressDeliverable } from "../lib/sourcing/delivery-terms"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"

/**
 * 新建采购单 / 供给分配统一作业面：工作台与独立建单页共享同一套高密度可编辑工作区。
 */
export type SalesOrderPaperPreviewRenderProps = Readonly<{
    salesOrderId: string | null
    title?: string
    open: boolean
    onOpenChange: (open: boolean) => void
}>

type PurchaseOrderCreatePageProps = {
    initialSalesOrderId?: string
    initialWorkItemId?: string
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
    /** 按销售单身份渲染原始单据预览；不得用供给投影凑纸。 */
    renderSalesOrderPreview?: (
        props: SalesOrderPaperPreviewRenderProps,
    ) => ReactNode
}

function cloneLines(lines: readonly SourcingLineInput[]): SourcingLineInput[] {
    return lines.map((line) => ({
        ...line,
        deliveryAllocations: line.deliveryAllocations?.map((item) => ({
            ...item,
        })),
    }))
}

export function PurchaseOrderCreatePage(props: PurchaseOrderCreatePageProps) {
    const profile = useAccountProfileQuery()
    const canCreate = hasPermission(
        profile.data?.permissions,
        "purchase_order:create",
    )
    if (profile.isPending) return <Skeleton className="h-40 w-full" />
    if (profile.isError || !canCreate) {
        return (
            <PageScaffold density="compact">
                <BusinessFailureState
                    kind={profile.isError ? "system" : "permission"}
                    title={profile.isError ? "权限信息加载失败" : "权限不足"}
                    description={
                        profile.isError
                            ? getErrorMessage(
                                  profile.error,
                                  "暂时无法核对采购单权限。",
                              )
                            : "当前账号不能新建采购单。"
                    }
                    action={
                        profile.isError ? (
                            <Button
                                id="purchase-order-create-retry-permissions"
                                type="button"
                                variant="outline"
                                onClick={() => void profile.refetch()}
                            >
                                重试
                            </Button>
                        ) : undefined
                    }
                />
            </PageScaffold>
        )
    }
    return <PurchaseOrderCreateSurface {...props} />
}

function PurchaseOrderCreateSurface({
    initialSalesOrderId = "",
    initialWorkItemId = "",
    embedded = false,
    onTaskCompleted,
    renderSalesOrderPreview,
}: PurchaseOrderCreatePageProps) {
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
    const profile = useAccountProfileQuery()
    const userId = profile.data?.userid ?? ""
    const sourcingCommand = useSourcingCommand(
        userId,
        createMutation.mutateAsync,
    )
    const submissionUnknown =
        sourcingCommand.pending !== null && !createMutation.isPending
    const [previewOpen, setPreviewOpen] = React.useState(false)
    const [linesResetToken, setLinesResetToken] = React.useState(0)
    const [checkedProductIds, setCheckedProductIds] = React.useState<
        ReadonlySet<string>
    >(() => new Set())
    const [visibleProductIds, setVisibleProductIds] = React.useState<
        readonly string[]
    >([])
    const [undoSnapshot, setUndoSnapshot] = React.useState<
        SourcingLineInput[] | null
    >(null)
    const [focusRequest, setFocusRequest] =
        React.useState<SourcingFocusRequest | null>(null)
    const [sourcePaper, setSourcePaper] = React.useState<{
        id: string
        title: string
    } | null>(null)
    const [actionError, setActionError] = React.useState<{
        error?: unknown
        title: string
        description: string
    } | null>(null)
    const createIntentRef = React.useRef<{
        fingerprint: string
        idempotencyKey: string
    } | null>(null)
    const draftScopeRef = React.useRef("")

    const workspace = React.useMemo(
        () => buildSourcingWorkspace(basesQuery.data ?? []),
        [basesQuery.data],
    )
    const workspaceRef = React.useRef(workspace)
    workspaceRef.current = workspace

    const submitSourcing = async (prepared?: SavedSourcingCommand) => {
        try {
            const { result, original } = await sourcingCommand.run(prepared)
            const order = {
                salesOrderId: original.input.salesOrderId,
                workItemId: original.input.workItemId,
            }

            if (result.status === "succeeded") {
                setPreviewOpen(false)
                createIntentRef.current = null
                const count = result.data.orders.length
                const stockCount = result.data.stockReservations.length
                const taskCompleted = result.data.workItemStatus === "COMPLETED"
                toast.add({
                    title: taskCompleted
                        ? "供给分配已完成"
                        : "本次供给分配已保存",
                    description: !taskCompleted
                        ? "您负责的商品仍有未分配数量，待办将继续保留，请继续分配剩余商品。"
                        : count === 0
                          ? `已锁定 ${stockCount} 项现货库存并生成仓库发货单，无需新建采购单。`
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
                    setLinesResetToken((token) => token + 1)
                }
                return
            }
            if (result.status === "failed") {
                createIntentRef.current = null
                if (result.code === "CONFLICT") {
                    await basesQuery.refetch()
                }
                setActionError({
                    title: original.hasUnknown
                        ? "上次提交结果确认中"
                        : "供给分配失败",
                    description: original.hasUnknown
                        ? `${result.message} 网络响应超时，系统正在确认上次提交是否成功，请先刷新核对避免重复建单。`
                        : result.code === "CONFLICT"
                          ? `${result.message} 创建依据已刷新，请核对后重试。`
                          : result.message,
                })
                return
            }
            setActionError({
                title: "上次提交结果确认中",
                description: `${result.message} 网络响应超时，系统正在确认上次提交是否成功，请先刷新核对避免重复建单。`,
            })
        } catch (error) {
            setActionError({
                error,
                title: "上次提交结果确认中",
                description: getErrorMessage(
                    error,
                    "网络响应超时，系统正在确认上次提交是否成功，请先刷新核对避免重复建单。",
                ),
            })
        }
    }

    const form = useAppForm({
        defaultValues: {
            salesOrderId: initialSalesOrderId,
            lines: [] as SourcingLineInput[],
        },
        listeners: {
            onChange: ({ formApi }) => {
                const order = workspaceRef.current.find(
                    (candidate) =>
                        candidate.salesOrderId ===
                        formApi.state.values.salesOrderId,
                )
                if (!userId || !order) return
                const key = sourcingDraftKey(
                    userId,
                    order.salesOrderId,
                    order.workItemId,
                )
                if (draftScopeRef.current !== key) return
                try {
                    writeInquiryLocalValue(key, formApi.state.values)
                } catch {
                    setActionError({
                        title: "本地输入未保存",
                        description:
                            "浏览器无法保存当前分配，请先完成本次办理再离开页面。",
                    })
                }
            },
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
            if (sourcingCommand.pending) {
                await submitSourcing()
                return
            }
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
                return [{ line, option, product }]
            })
            const fingerprint = JSON.stringify({
                salesOrderId: order.salesOrderId,
                workItemId: order.workItemId,
                lines: selected.map(({ line, option, product }) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    basisId: line.basisId,
                    sourceType: option.sourceType,
                    targetWarehouseId: line.targetWarehouseId || undefined,
                    quantity: line.quantity.trim(),
                    expectedDeliveryDate: line.expectedDeliveryDate,
                    deliveryAllocations: sourcingDeliveryAllocations(
                        product,
                        line,
                    ),
                })),
            })
            if (createIntentRef.current?.fingerprint !== fingerprint) {
                createIntentRef.current = {
                    fingerprint,
                    idempotencyKey: `create-sourcing:${order.salesOrderId}:${crypto.randomUUID()}`,
                }
            }
            const input = {
                workItemId: order.workItemId,
                salesOrderId: order.salesOrderId,
                lines: selected.map(({ line, option, product }) => ({
                    salesOrderLineId: line.salesOrderLineId,
                    basisId: line.basisId,
                    sourceType: option.sourceType,
                    targetWarehouseId: line.targetWarehouseId || undefined,
                    quantity: line.quantity.trim(),
                    expectedDeliveryDate: line.expectedDeliveryDate,
                    deliveryAllocations: sourcingDeliveryAllocations(
                        product,
                        line,
                    ),
                })),
                idempotencyKey: createIntentRef.current.idempotencyKey,
            }
            await submitSourcing({
                input,
                salesOrderNo: order.salesOrderNo,
                formValues: value,
            })
        },
    })

    React.useEffect(() => {
        if (initialSalesOrderId) return
        const current = form.state.values.salesOrderId
        if (current) return
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
    const selectedOrderRef = React.useRef(selectedOrder)
    selectedOrderRef.current = selectedOrder
    const selectedOrderKey = selectedOrder
        ? `${selectedOrder.salesOrderId}:${selectedOrder.workItemId}`
        : ""

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
                form.setFieldValue(
                    `lines[${index}].deliveryAllocations`,
                    line.deliveryAllocations,
                )
            })
        },
        [form],
    )

    const initializedLinesKey = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (basesQuery.isPending || !selectedOrderRef.current || !userId) return
        const key = `${userId}:${selectedOrderKey}:${linesResetToken}`
        if (initializedLinesKey.current === key) return
        initializedLinesKey.current = key
        const draftKey = sourcingDraftKey(
            userId,
            selectedOrderRef.current.salesOrderId,
            selectedOrderRef.current.workItemId,
        )
        const saved =
            readInquiryLocalValue<PurchaseOrderCreateFormValues>(draftKey)
        draftScopeRef.current = draftKey
        writeLines(
            saved?.salesOrderId === selectedSalesOrderId
                ? saved.lines
                : buildDefaultSourcingLines(selectedOrderRef.current),
        )
        setCheckedProductIds(new Set())
        setUndoSnapshot(null)
        setPreviewOpen(false)
    }, [
        basesQuery.isPending,
        selectedOrderKey,
        selectedSalesOrderId,
        linesResetToken,
        writeLines,
        userId,
    ])

    const lines = useStore(form.store, (state) => state.values.lines)
    const onSetParticipation = useSourcingParticipation(
        form as unknown as PurchaseOrderCreateFormApi,
        `${selectedOrderKey}:${linesResetToken}`,
    )
    const editorRows = React.useMemo(
        () =>
            selectedOrder ? buildSourcingEditorRows(selectedOrder, lines) : [],
        [selectedOrder, lines],
    )

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

    // 批量选区（UI 临时勾选），仅操作 checkedProductIds，与「是否参与本次提交」完全隔离。
    const toggleCheckedProducts = React.useCallback(
        (ids: readonly string[], checked: boolean) => {
            setCheckedProductIds((current) => {
                const next = new Set(current)
                for (const id of ids) {
                    if (checked) next.add(id)
                    else next.delete(id)
                }
                return next
            })
        },
        [],
    )

    const clearCheckedProducts = React.useCallback(() => {
        setCheckedProductIds(new Set())
    }, [])

    const checkedRows = React.useMemo(
        () =>
            editorRows.filter((row) =>
                checkedProductIds.has(row.product.salesOrderLineId),
            ),
        [checkedProductIds, editorRows],
    )

    const recordUndoAndWrite = React.useCallback(
        (next: SourcingLineInput[]) => {
            setUndoSnapshot(cloneLines(lines))
            writeLines(next)
        },
        [lines, writeLines],
    )

    const handleUndo = React.useCallback(() => {
        if (!undoSnapshot) return
        writeLines(undoSnapshot)
        setUndoSnapshot(null)
        toast.add({
            title: "已撤销上次批量更改",
            type: "info",
            timeout: 3000,
        })
    }, [undoSnapshot, writeLines])

    const batchApplySource = React.useCallback(
        (basisId: string, warehouse?: { id: string; name: string }) => {
            if (!selectedOrder || checkedProductIds.size === 0) return
            const replaced = new Set<string>()
            const next = lines.flatMap((line) => {
                if (!checkedProductIds.has(line.salesOrderLineId)) return [line]
                const product = selectedOrder.lines.find(
                    (item) => item.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, basisId)
                if (!option) return [line]
                if (replaced.has(line.salesOrderLineId)) return []
                replaced.add(line.salesOrderLineId)
                const needsWarehouse =
                    option.sourceType === "PURCHASE" &&
                    option.fulfillmentResponsibility === "WAREHOUSE"
                return [
                    {
                        ...line,
                        selected: lines.some(
                            (candidate) =>
                                candidate.salesOrderLineId ===
                                    line.salesOrderLineId && candidate.selected,
                        ),
                        basisId,
                        quantity: option.maxCreateQuantity,
                        expectedDeliveryDate: option.expectedDeliveryDate,
                        deliveryAllocations: undefined,
                        targetWarehouseId: needsWarehouse
                            ? (warehouse?.id ?? line.targetWarehouseId ?? "")
                            : "",
                        targetWarehouseName: needsWarehouse
                            ? (warehouse?.name ??
                              line.targetWarehouseName ??
                              "")
                            : "",
                    },
                ]
            })
            recordUndoAndWrite(next)
            setCheckedProductIds(new Set())
            toast.add({
                title: "已批量指定供给方案",
                description: `已更新 ${replaced.size} 个商品，其余不支持该方案的商品保持原状。`,
                type: "success",
                timeout: 4000,
            })
        },
        [checkedProductIds, lines, recordUndoAndWrite, selectedOrder],
    )

    const batchApplyWarehouse = React.useCallback(
        (warehouse: { id: string; name: string }) => {
            if (!selectedOrder || checkedProductIds.size === 0) return
            let updated = 0
            const next = lines.map((line) => {
                if (!checkedProductIds.has(line.salesOrderLineId)) return line
                const product = selectedOrder.lines.find(
                    (item) => item.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, line.basisId)
                if (
                    option?.sourceType !== "PURCHASE" ||
                    option.fulfillmentResponsibility !== "WAREHOUSE"
                ) {
                    return line
                }
                updated += 1
                return {
                    ...line,
                    targetWarehouseId: warehouse.id,
                    targetWarehouseName: warehouse.name,
                }
            })
            if (updated === 0) return
            recordUndoAndWrite(next)
            setCheckedProductIds(new Set())
            toast.add({
                title: `已为 ${updated} 条入仓明细设置目标仓`,
                description: warehouse.name || undefined,
                type: "success",
                timeout: 4000,
            })
        },
        [checkedProductIds, lines, recordUndoAndWrite, selectedOrder],
    )

    const batchApplyDeliveryDate = React.useCallback(
        (date: string, clampToDeadline: boolean) => {
            if (!selectedOrder || checkedProductIds.size === 0) return
            let updated = 0
            const next = lines.map((line) => {
                if (!checkedProductIds.has(line.salesOrderLineId)) return line
                const product = selectedOrder.lines.find(
                    (item) => item.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, line.basisId)
                if (option?.sourceType !== "PURCHASE") return line
                const deadline = product?.deliveryDeadline?.trim()
                const resolvedDate =
                    clampToDeadline && deadline && date > deadline
                        ? deadline
                        : date
                updated += 1
                return { ...line, expectedDeliveryDate: resolvedDate }
            })
            if (updated === 0) return
            recordUndoAndWrite(next)
            setCheckedProductIds(new Set())
            toast.add({
                title: `已批量修改 ${updated} 条采购交期`,
                type: "success",
                timeout: 4000,
            })
        },
        [checkedProductIds, lines, recordUndoAndWrite, selectedOrder],
    )

    const batchSetParticipation = React.useCallback(
        (included: boolean) => {
            if (checkedProductIds.size === 0) return
            setUndoSnapshot(cloneLines(lines))
            onSetParticipation([...checkedProductIds], included)
            setCheckedProductIds(new Set())
            toast.add({
                title: included
                    ? `已将 ${checkedProductIds.size} 个商品纳入本次分配`
                    : `已将 ${checkedProductIds.size} 个商品设为暂不分配`,
                type: "success",
                timeout: 3500,
            })
        },
        [checkedProductIds, lines, onSetParticipation],
    )

    const syncWarehouseToOthers = React.useCallback(
        (
            warehouseId: string,
            warehouseName: string,
            excludeSalesOrderLineId: string,
        ) => {
            if (!selectedOrder || !warehouseId) return
            let updated = 0
            const next = lines.map((line) => {
                if (
                    !line.selected ||
                    line.salesOrderLineId === excludeSalesOrderLineId
                ) {
                    return line
                }
                const product = selectedOrder.lines.find(
                    (item) => item.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, line.basisId)
                if (
                    option?.sourceType !== "PURCHASE" ||
                    option.fulfillmentResponsibility !== "WAREHOUSE"
                ) {
                    return line
                }
                updated += 1
                return {
                    ...line,
                    targetWarehouseId: warehouseId,
                    targetWarehouseName: warehouseName,
                }
            })
            if (updated === 0) return
            recordUndoAndWrite(next)
            toast.add({
                title: `已将目标仓同步到其余 ${updated} 条入仓明细`,
                description: warehouseName || undefined,
                type: "success",
                timeout: 3500,
            })
        },
        [lines, recordUndoAndWrite, selectedOrder],
    )

    const applyBestSourcingOptions = React.useCallback(() => {
        if (!selectedOrder) return
        const next = assignBestSourcingOptions(selectedOrder, lines)
        recordUndoAndWrite(next)
        const filled = next.filter(
            (line) => line.selected && line.basisId,
        ).length
        toast.add({
            title: filled > 0 ? "已重新分配供给" : "没有可匹配的供给方案",
            description:
                filled > 0
                    ? "已为本次分配的商品优先分配现有库存，采购缺口优先推荐供应商直发；暂不分配的商品保持原方案。"
                    : "当前明细没有可用库存或合格采购供给。",
            type: filled > 0 ? "success" : "warning",
            timeout: 4000,
        })
    }, [selectedOrder, lines, recordUndoAndWrite])

    const applyInquirySourcing = React.useCallback(() => {
        if (!selectedOrder) return
        let matched = 0
        const next = lines.map((line) => {
            if (!line.selected) return line
            const product = selectedOrder.lines.find(
                (item) => item.salesOrderLineId === line.salesOrderLineId,
            )
            if (
                !product ||
                findSourcingOption(product, line.basisId)?.sourceType ===
                    "EXISTING_STOCK"
            )
                return line
            const candidates = product.options.filter((option) => {
                if (
                    !option.inquirySource ||
                    (option.priceBasis === "BULK" &&
                        option.fulfillmentResponsibility === "SUPPLIER_DIRECT")
                )
                    return false
                try {
                    return (
                        compareDecimal(
                            option.maxCreateQuantity,
                            line.quantity,
                            6,
                        ) >= 0
                    )
                } catch {
                    return false
                }
            })
            const option = pickBestSourcingOption(
                candidates,
                line.quantity,
                product,
            )
            if (!option) return line
            matched += 1
            return {
                ...line,
                basisId: option.basisId,
                expectedDeliveryDate: option.expectedDeliveryDate,
                deliveryAllocations: undefined,
                targetWarehouseId:
                    option.fulfillmentResponsibility === "WAREHOUSE"
                        ? line.targetWarehouseId
                        : "",
                targetWarehouseName:
                    option.fulfillmentResponsibility === "WAREHOUSE"
                        ? line.targetWarehouseName
                        : "",
            }
        })
        if (matched > 0) recordUndoAndWrite(next)
        toast.add({
            title: matched
                ? `已带入 ${matched} 条询价报价候选`
                : "当前没有可沿用的询价报价",
            description: matched
                ? "现有库存分配已保留。请继续核对供应商、数量、配送范围、交期及付款条件；集采直发在明细中明确选择。"
                : "报价可能已失效、数量不满足条件或需明确选择集采直发。请在明细中核对其他供给。",
            type: matched ? "success" : "warning",
        })
    }, [selectedOrder, lines, recordUndoAndWrite])

    const addSplitLine = React.useCallback(
        (salesOrderLineId: string) => {
            if (!selectedOrder) return
            const product = selectedOrder.lines.find(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (!product) return
            const liveLines = form.state.values.lines
            const allocationCount = liveLines.filter(
                (line) => line.salesOrderLineId === salesOrderLineId,
            ).length
            if (!canSplitSourcingProduct(product, allocationCount)) return
            const used = new Set(
                liveLines
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
            const lastIndex = liveLines.findLastIndex(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            const next = [...liveLines]
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
        [form, selectedOrder, writeLines],
    )

    /** 按用户选定的精确履约方案新增拆分来源，智能带入剩余缺口或可用地址，返回新增 rowKey。 */
    const addSplitLineWithOption = React.useCallback(
        (salesOrderLineId: string, basisId: string): string | undefined => {
            if (!selectedOrder) return undefined
            const product = selectedOrder.lines.find(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (!product) return undefined
            const liveLines = form.state.values.lines
            const existingLines = liveLines.filter(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (!canSplitSourcingProduct(product, existingLines.length)) {
                return undefined
            }
            if (existingLines.some((line) => line.basisId === basisId)) {
                return undefined
            }
            const option = findSourcingOption(product, basisId)
            if (!option) return undefined

            let residualQty = product.remainingQuantity
            try {
                const allocated = sumFixed(
                    existingLines
                        .filter((line) => line.selected)
                        .map((line) => line.quantity || "0"),
                    { maxScale: 6, outputScale: 6 },
                )
                const diff = subtractFixed(
                    product.remainingQuantity,
                    allocated,
                    { maxScale: 6, outputScale: 6 },
                )
                residualQty =
                    compareDecimal(diff, "0", 6) > 0 ? compactFixed(diff) : "0"
            } catch {
                residualQty = "0"
            }

            const newRowKey = `${salesOrderLineId}:${crypto.randomUUID()}`
            const draftLine: SourcingLineInput = {
                rowKey: newRowKey,
                salesOrderLineId,
                selected: true,
                quantity: "",
                basisId: option.basisId,
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: option.expectedDeliveryDate,
            }

            if (usesAddressQuantity(product, draftLine)) {
                draftLine.deliveryAllocations = []
                draftLine.quantity = "0"
                if (compareDecimal(residualQty, "0", 6) > 0) {
                    let cappedResidual = residualQty
                    try {
                        if (
                            compareDecimal(
                                residualQty,
                                option.maxCreateQuantity,
                                6,
                            ) > 0
                        ) {
                            cappedResidual = option.maxCreateQuantity
                        }
                    } catch {
                        cappedResidual = option.maxCreateQuantity
                    }
                    const probeLines = [...liveLines, draftLine]
                    const filled = buildFillDirectShortfallPatches(
                        product,
                        probeLines,
                        newRowKey,
                        cappedResidual,
                    )
                    const patched = filled.patches.find(
                        (patch) => patch.nextLine.rowKey === newRowKey,
                    )?.nextLine
                    if (patched) {
                        draftLine.deliveryAllocations =
                            patched.deliveryAllocations
                        draftLine.quantity = patched.quantity
                    }
                }
            } else if (compareDecimal(residualQty, "0", 6) > 0) {
                try {
                    draftLine.quantity =
                        compareDecimal(
                            residualQty,
                            option.maxCreateQuantity,
                            6,
                        ) <= 0
                            ? residualQty
                            : option.maxCreateQuantity
                } catch {
                    draftLine.quantity = option.maxCreateQuantity
                }
            }

            const lastIndex = liveLines.findLastIndex(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            const next = [...liveLines]
            next.splice(lastIndex + 1, 0, draftLine)
            writeLines(next)
            return newRowKey
        },
        [form, selectedOrder, writeLines],
    )

    /** 一键将剩余缺口（或指定/剩余未分配地址）拆给新来源，返回新增 rowKey。 */
    const addResidualSplitLine = React.useCallback(
        (
            salesOrderLineId: string,
            options?: { moveAddressKeys?: readonly string[] },
        ): string | undefined => {
            if (!selectedOrder) return undefined
            const product = selectedOrder.lines.find(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (!product) return undefined
            const liveLines = form.state.values.lines
            const existingLines = liveLines.filter(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (!canSplitSourcingProduct(product, existingLines.length)) {
                return undefined
            }
            const used = new Set(
                existingLines.map((line) => line.basisId).filter(Boolean),
            )
            const candidates = product.options.filter(
                (candidate) => !used.has(candidate.basisId),
            )
            if (candidates.length === 0) {
                toast.add({
                    title: "没有更多履约方案",
                    description: "该销售明细的可选履约方案都已添加。",
                    type: "warning",
                    timeout: 4000,
                })
                return undefined
            }

            const moveKeys = options?.moveAddressKeys?.length
                ? new Set(options.moveAddressKeys)
                : null

            // 若指定了要迁出的地址集合，先从同商品现有直发来源中释放这些地址
            const baseLines = moveKeys
                ? liveLines.map((line) => {
                      if (
                          line.salesOrderLineId !== salesOrderLineId ||
                          !usesAddressQuantity(product, line)
                      ) {
                          return line
                      }
                      const currentRows =
                          line.deliveryAllocations ??
                          sourcingDeliveryAllocations(product, line)
                      const nextRows = currentRows.filter(
                          (row) =>
                              !moveKeys.has(
                                  JSON.stringify([
                                      row.recipient,
                                      row.phone,
                                      row.region_code,
                                      row.address,
                                  ]),
                              ),
                      )
                      return {
                          ...line,
                          deliveryAllocations: nextRows,
                          quantity: deliveryQuantity(nextRows),
                      }
                  })
                : liveLines

            const updatedExistingLines = baseLines.filter(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )

            // 计算剩余未分配总量
            let residualQty = product.remainingQuantity
            try {
                const allocated = sumFixed(
                    updatedExistingLines
                        .filter((line) => line.selected)
                        .map((line) => line.quantity || "0"),
                    { maxScale: 6, outputScale: 6 },
                )
                const diff = subtractFixed(
                    product.remainingQuantity,
                    allocated,
                    { maxScale: 6, outputScale: 6 },
                )
                residualQty =
                    compareDecimal(diff, "0", 6) > 0 ? compactFixed(diff) : "0"
            } catch {
                residualQty = ""
            }

            const targetAvailableRows = availableDeliveryRows(
                product,
                {
                    rowKey: "__probe__",
                    salesOrderLineId,
                    selected: true,
                    quantity: "",
                    basisId: candidates[0]!.basisId,
                    expectedDeliveryDate: candidates[0]!.expectedDeliveryDate,
                },
                baseLines,
            ).filter((addr) => {
                if (compareDecimal(addr.quantity, "0", 6) <= 0) return false
                if (!moveKeys) return true
                return moveKeys.has(
                    JSON.stringify([
                        addr.recipient,
                        addr.phone,
                        addr.region_code,
                        addr.address,
                    ]),
                )
            })

            // 优先选择能覆盖最多待分配/指定地址的候选方案（避免拆给同样不配送的供应商）
            const preferredCandidates =
                targetAvailableRows.length > 0
                    ? (() => {
                          const scored = candidates.map((candidate) => {
                              const deliverableCount =
                                  candidate.sourceType === "EXISTING_STOCK" ||
                                  candidate.fulfillmentResponsibility ===
                                      "WAREHOUSE"
                                      ? targetAvailableRows.length
                                      : targetAvailableRows.filter((addr) =>
                                            addressDeliverable(
                                                candidate,
                                                addr.region_code,
                                            ),
                                        ).length
                              return { candidate, deliverableCount }
                          })
                          const bestCoverage = Math.max(
                              ...scored.map((item) => item.deliverableCount),
                          )
                          return bestCoverage > 0
                              ? scored
                                    .filter(
                                        (item) =>
                                            item.deliverableCount ===
                                            bestCoverage,
                                    )
                                    .map((item) => item.candidate)
                              : candidates
                      })()
                    : candidates

            const option =
                pickBestSourcingOption(
                    preferredCandidates,
                    residualQty,
                    product,
                ) ?? preferredCandidates[0]!
            const newRowKey = `${salesOrderLineId}:${crypto.randomUUID()}`
            const draftLine: SourcingLineInput = {
                rowKey: newRowKey,
                salesOrderLineId,
                selected: true,
                quantity: "",
                basisId: option.basisId,
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: option.expectedDeliveryDate,
            }

            // 若新来源是带地址直发，自动承接指定（或全部未分配）且本源可配送的地址
            if (usesAddressQuantity(product, draftLine)) {
                const assignable = targetAvailableRows.filter((addr) =>
                    addressDeliverable(option, addr.region_code),
                )
                draftLine.deliveryAllocations = assignable
                draftLine.quantity = deliveryQuantity(assignable)
            } else if (residualQty && compareDecimal(residualQty, "0", 6) > 0) {
                try {
                    draftLine.quantity =
                        compareDecimal(
                            residualQty,
                            option.maxCreateQuantity,
                            6,
                        ) <= 0
                            ? residualQty
                            : option.maxCreateQuantity
                } catch {
                    draftLine.quantity = option.maxCreateQuantity
                }
            }

            const lastIndex = baseLines.findLastIndex(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            const next = [...baseLines]
            next.splice(lastIndex + 1, 0, draftLine)
            writeLines(next)
            return newRowKey
        },
        [form, selectedOrder, writeLines],
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

    const restoreProductLines = React.useCallback(
        (salesOrderLineId: string, snapshot: SourcingLineInput[]) => {
            const firstIndex = lines.findIndex(
                (line) => line.salesOrderLineId === salesOrderLineId,
            )
            if (firstIndex < 0) return
            const withoutProduct = lines.filter(
                (line) => line.salesOrderLineId !== salesOrderLineId,
            )
            const next = [...withoutProduct]
            next.splice(firstIndex, 0, ...cloneLines(snapshot))
            writeLines(next)
            toast.add({
                title: "已还原当前商品的修改",
                type: "info",
                timeout: 2500,
            })
        },
        [lines, writeLines],
    )

    const canMatchBest = Boolean(
        selectedOrder?.lines.some(
            (product) =>
                product.options.length > 0 &&
                lines.some(
                    (line) =>
                        line.salesOrderLineId === product.salesOrderLineId &&
                        line.selected,
                ),
        ),
    )

    const blockingRows = React.useMemo(
        () =>
            editorRows.filter(
                (row) =>
                    (row.selected || row.partiallySelected) &&
                    row.issues.length > 0,
            ),
        [editorRows],
    )

    const locateProduct = React.useCallback(
        (
            salesOrderLineId: string,
            options?: {
                openDialog?: boolean
                section?: "compare" | "addresses"
            },
        ) => {
            setFocusRequest({
                salesOrderLineId,
                openDialog: options?.openDialog,
                section: options?.section,
                token: Date.now(),
            })
        },
        [],
    )

    const openPreview = React.useCallback(async () => {
        if (sourcingCommand.pending) {
            setPreviewOpen(true)
            return
        }
        for (const field of Object.values(form.fieldInfo)) {
            const instance = field?.instance
            if (!instance || instance.store.state.meta.isTouched) continue
            instance.setMeta((prev) => ({ ...prev, isTouched: true }))
        }
        await form.validate("submit")
        setActionError(null)
        if (form.state.canSubmit) {
            setPreviewOpen(true)
            return
        }
        // 自动定位到第一条阻塞错误行，避免用户大海捞针
        const firstBroken = blockingRows[0]
        if (firstBroken) {
            locateProduct(firstBroken.product.salesOrderLineId, {
                openDialog: false,
            })
        }
        const description =
            collectSourcingErrorMessages(form.getAllErrors()).join("；") ||
            "请检查本次分配数量和供给方案后重试。"
        toast.add({
            title: "无法预览供给分配",
            description,
            type: "error",
            timeout: 6000,
        })
    }, [blockingRows, form, locateProduct, sourcingCommand.pending])

    const pendingRecovery = sourcingCommand.pending ? (
        <Alert variant="warning">
            <AlertTitle>上次提交结果确认中</AlertTitle>
            <AlertDescription>
                <p>
                    {sourcingCommand.pending.salesOrderNo} ·{" "}
                    {sourcingCommand.pending.input.lines.length}{" "}
                    条分配已保留。网络响应超时，系统正在确认上次提交是否成功，请先刷新核对避免重复建单。
                </p>
                <Button
                    id="procurement-sourcing-recover-saved-command"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={createMutation.isPending}
                    onClick={() => void submitSourcing()}
                >
                    {createMutation.isPending
                        ? "正在核对…"
                        : "核对上次提交结果"}
                </Button>
            </AlertDescription>
        </Alert>
    ) : null

    if (basesQuery.isPending) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                {embedded ? null : (
                    <ListWorkspaceHeader
                        className="bg-transparent pb-3"
                        title="货源分配与采购建单"
                        description="正在加载库存与采购供给…"
                    />
                )}
                <div
                    className="flex flex-col gap-3"
                    aria-busy="true"
                    aria-label="加载中"
                >
                    {pendingRecovery}
                    <Skeleton className="h-16" />
                    <Skeleton className="h-40" />
                </div>
            </PageScaffold>
        )
    }

    if (basesQuery.isError && !basesQuery.data) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                {embedded ? null : (
                    <ListWorkspaceHeader
                        className="bg-transparent pb-3"
                        title="货源分配与采购建单"
                        description="待采购商品及报价加载失败"
                    />
                )}
                {pendingRecovery}
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

    const inquiryUnavailableReason = !selectedOrder
        ? "请先选择需要分配供给的销售单。"
        : !selectedOrder.lines.some((line) =>
                line.options.some((option) => option.inquirySource),
            )
          ? selectedOrder.lines.some((line) =>
                line.options.some((option) => option.hasSourceInquiry),
            )
              ? "销售单已关联询价，但当前没有可沿用的有效采购报价。请核对来源反馈轮次的内部报价、数量、有效期和供给状态。"
              : "当前可选供给没有关联询价报价，请核对销售单是否从询价单带入；已有库存分配可继续保留。"
          : !lines.some((line) => line.selected)
            ? "请先勾选需要分配的商品，再沿用询价结果。"
            : undefined

    const utilityActions = (
        <PurchaseOrderCreateUtilityActions
            matchDisabled={!canMatchBest}
            onMatchBest={applyBestSourcingOptions}
            onReuseInquiry={applyInquirySourcing}
            inquiryUnavailableReason={inquiryUnavailableReason}
            canUndo={Boolean(undoSnapshot)}
            onUndo={handleUndo}
        />
    )

    const selectionBar =
        selectedOrder && checkedRows.length > 0 ? (
            <PurchaseOrderCreateSelectionBar
                checkedRows={checkedRows}
                visibleRowIds={visibleProductIds}
                onSelectAllVisible={() =>
                    toggleCheckedProducts(visibleProductIds, true)
                }
                onClearSelection={clearCheckedProducts}
                onBatchApplySource={batchApplySource}
                onBatchApplyWarehouse={batchApplyWarehouse}
                onBatchApplyDeliveryDate={batchApplyDeliveryDate}
                onBatchSetParticipation={batchSetParticipation}
            />
        ) : undefined

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
            value: (
                <MoneyValue
                    value={previewTotals.gross}
                    display={embedded ? "exact" : "full"}
                />
            ),
        },
    ]

    const blockingBadge =
        blockingRows.length > 0 ? (
            <Button
                id="procurement-orders-create-locate-blocking"
                type="button"
                variant="outline"
                size="sm"
                className="border-destructive-border bg-destructive-soft/60 text-destructive-soft-foreground hover:bg-destructive-soft"
                onClick={() =>
                    locateProduct(blockingRows[0]!.product.salesOrderLineId, {
                        openDialog: false,
                    })
                }
            >
                <TriangleAlertIcon aria-hidden="true" />
                {blockingRows.length} 项待完善 · 定位
            </Button>
        ) : null

    const previewAction = (
        <div className="flex flex-wrap items-center gap-2">
            {blockingBadge}
            <Button
                id="procurement-orders-create-preview"
                type="button"
                data-testid="purchase-create-preview"
                onClick={() => void openPreview()}
            >
                预览分单结果
            </Button>
        </div>
    )

    return (
        <SourcingOptionsRefreshContext
            value={() => basesQuery.refetch({ throwOnError: true })}
        >
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded
                        ? cn(
                              workspaceEmbeddedScaffoldClassName,
                              "overflow-hidden",
                          )
                        : "pb-8"
                }
            >
                {embedded ? null : (
                    <ListWorkspaceHeader
                        className="bg-transparent pb-3"
                        title="货源分配与采购建单"
                    >
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
                    </ListWorkspaceHeader>
                )}

                <div
                    className={
                        embedded
                            ? "flex min-h-0 flex-1 flex-col overflow-hidden"
                            : undefined
                    }
                >
                    {pendingRecovery}
                    <SystemFeedback
                        problemId="purchase-allocation"
                        operation
                        error={actionError?.error ?? actionError?.description}
                        title={actionError?.title ?? "供给分配未完成"}
                        description={actionError?.description}
                    />

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
                                embedded ? "min-h-0 flex-1 gap-0" : "gap-4",
                            )}
                            onSubmit={(event) => {
                                event.preventDefault()
                                event.stopPropagation()
                            }}
                        >
                            {embedded ? null : (
                                <PurchaseOrderCreateSourcePanel
                                    workspace={workspace}
                                    selectedSalesOrderId={selectedSalesOrderId}
                                    selectedOrder={selectedOrder}
                                    disabled={Boolean(initialSalesOrderId)}
                                    flat={false}
                                    salesOrderReturnTo={salesOrderReturnTo}
                                    onPreviewSalesOrder={
                                        renderSalesOrderPreview
                                            ? (id, title) =>
                                                  setSourcePaper({ id, title })
                                            : undefined
                                    }
                                    onSalesOrderChange={(value) =>
                                        form.setFieldValue(
                                            "salesOrderId",
                                            value,
                                        )
                                    }
                                />
                            )}

                            {selectedOrder ? (
                                sourcingFormLinesReady(lines, selectedOrder) ? (
                                    <div
                                        className={cn(
                                            embedded
                                                ? "flex min-h-0 flex-1 flex-col overflow-hidden"
                                                : "overflow-hidden rounded-lg border border-border bg-card",
                                        )}
                                    >
                                        <PurchaseOrderCreateSourcingWorkspace
                                            key={selectedOrder.salesOrderId}
                                            form={
                                                form as unknown as PurchaseOrderCreateFormApi
                                            }
                                            order={selectedOrder}
                                            onAddSplit={addSplitLine}
                                            onAddSplitWithOption={
                                                addSplitLineWithOption
                                            }
                                            onAddResidualSplit={
                                                addResidualSplitLine
                                            }
                                            onRemoveSplit={removeSplitLine}
                                            toolbarActions={utilityActions}
                                            selectionBar={selectionBar}
                                            checkedProductIds={
                                                checkedProductIds
                                            }
                                            onToggleChecked={
                                                toggleCheckedProducts
                                            }
                                            onClearChecked={
                                                clearCheckedProducts
                                            }
                                            onSetParticipation={
                                                onSetParticipation
                                            }
                                            onSyncWarehouseToOthers={
                                                syncWarehouseToOthers
                                            }
                                            onRestoreProductLines={
                                                restoreProductLines
                                            }
                                            onVisibleIdsChange={
                                                setVisibleProductIds
                                            }
                                            focusRequest={focusRequest}
                                            scrollable={embedded}
                                        />
                                    </div>
                                ) : (
                                    <Skeleton className="h-40" />
                                )
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
                        summary={
                            <PurchaseOrderCreateFooterTotals
                                items={sourcingTotalItems.filter(
                                    (item) => item.id !== "lines",
                                )}
                            />
                        }
                        fallback={
                            <div
                                className={cn(
                                    workspaceTaskSurfacePadClassName,
                                    "flex shrink-0 flex-col items-stretch gap-3 border-t border-border/40 py-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4",
                                )}
                            >
                                <PurchaseOrderCreateFooterTotals
                                    items={sourcingTotalItems.filter(
                                        (item) => item.id !== "lines",
                                    )}
                                />
                                <div className="flex shrink-0 justify-end">
                                    {previewAction}
                                </div>
                            </div>
                        }
                    >
                        {previewAction}
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
                    scopeRows={editorRows}
                    creating={createMutation.isPending}
                    unresolved={submissionUnknown}
                    actionError={actionError}
                    description={confirmationDescription}
                    onOpenChange={setPreviewOpen}
                    onLocateProduct={(salesOrderLineId) =>
                        locateProduct(salesOrderLineId, { openDialog: true })
                    }
                    onConfirm={() => {
                        if (sourcingCommand.pending) void submitSourcing()
                        else void form.handleSubmit()
                    }}
                />
            </PageScaffold>
        </SourcingOptionsRefreshContext>
    )
}

function PurchaseOrderCreateFooterTotals({
    items,
}: {
    items: readonly {
        id: string
        label: string
        value: ReactNode
    }[]
}) {
    return (
        <div className="min-w-0">
            <dl className="flex flex-wrap items-baseline gap-x-5 gap-y-1 text-xs">
                {items.map((item) => (
                    <div
                        key={item.id}
                        className="flex shrink-0 items-baseline gap-1.5 whitespace-nowrap"
                    >
                        <dt className="text-xs text-muted-foreground">
                            {item.label}
                        </dt>
                        <dd className="num text-body-compact font-medium">
                            {item.value}
                        </dd>
                    </div>
                ))}
            </dl>
        </div>
    )
}
