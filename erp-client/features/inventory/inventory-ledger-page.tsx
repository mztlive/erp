"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    DownloadIcon,
    RefreshCwIcon,
    SearchIcon,
    SlashIcon,
} from "lucide-react"
import type { PaginationState } from "@tanstack/react-table"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionConfirmDialog,
    FormalActionResult,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    OptionCombobox,
    PageActions,
    PageHeader,
    PageScaffold,
    surfaceInsetClassName,
} from "@/components/business"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { useAppForm } from "@/components/form"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Input } from "@/components/ui/input"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { InventoryBalancePreview } from "@/features/inventory/inventory-balance-preview"
import { useInventoryColumns } from "@/features/inventory/inventory-columns"
import {
    adjustSchema,
    ChipFilter,
    defaultSortValue,
    localNowInput,
    MOVEMENT_TYPE_OPTIONS,
    parseAvailability,
    parseView,
    sortOptions,
} from "@/features/inventory/presentation"
import {
    useBalanceDetailQuery,
    useCreateAdjustmentDraftMutation,
    useInventoryListQuery,
    useResolveAdjustmentUnknownMutation,
    useStartInventoryExportMutation,
    useSubmitAdjustmentMutation,
} from "@/features/inventory/queries"
import type {
    AdjustmentReasonType,
    InventoryAvailability,
    InventoryQuery,
    InventoryView,
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"
import {
    AVAILABILITY_LABEL,
    REASON_TYPE_OPTIONS,
    VIEW_LABEL,
} from "@/features/inventory/types"
import {
    decodeInventoryCursor,
    encodeInventoryCursor,
} from "@/features/inventory/cursor"
import type { InventoryExportJob } from "@/features/inventory/api"
import { resultText } from "@/lib/ui-text"

export function InventoryLedgerPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    // §10: 375 窄屏只读；平板仍可发起调整
    const isPhoneNarrow = React.useSyncExternalStore(
        (onChange) => {
            const mq = window.matchMedia("(max-width: 480px)")
            mq.addEventListener("change", onChange)
            return () => mq.removeEventListener("change", onChange)
        },
        () => window.matchMedia("(max-width: 480px)").matches,
        () => false,
    )

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const warehouseId = searchParams.get("warehouseId") ?? undefined
    const skuId = searchParams.get("skuId") ?? undefined
    const salesOrderLineId = searchParams.get("salesOrderLineId") ?? undefined
    const availability = parseAvailability(searchParams.get("availability"))
    const balanceIdParam = searchParams.get("balanceId") ?? undefined
    const adjustmentIdParam = searchParams.get("adjustmentId") ?? undefined
    const movementTypeParam = searchParams.get("movementType") ?? ""
    const movementType = React.useMemo(
        () => movementTypeParam.split(",").filter(Boolean),
        [movementTypeParam],
    )
    const occurredFrom = searchParams.get("occurredFrom") ?? undefined
    const occurredTo = searchParams.get("occurredTo") ?? undefined
    const sortValue = searchParams.get("sort") ?? defaultSortValue(view)
    const pageSizeParam = Number(searchParams.get("pageSize") ?? "20")
    const pageSize =
        Number.isSafeInteger(pageSizeParam) && pageSizeParam > 0
            ? Math.min(pageSizeParam, 100)
            : 20
    const cursorParam = searchParams.get("cursor") ?? undefined
    const cursorOffset = decodeInventoryCursor(cursorParam, view)

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.floor(cursorOffset / pageSize),
        pageSize,
    })
    const [previewBalanceId, setPreviewBalanceId] = React.useState<
        string | null
    >(balanceIdParam ?? null)
    const [, setAdjustBalanceId] = React.useState<string | null>(null)
    const [adjustDraftId, setAdjustDraftId] = React.useState<string | null>(
        null,
    )
    const [adjustLockVersion, setAdjustLockVersion] = React.useState<number>(0)
    const [adjustSeedLock, setAdjustSeedLock] = React.useState<number>(0)
    const [adjustMeta, setAdjustMeta] = React.useState<{
        warehouseName: string
        skuCode: string
        skuName: string
        baseUnit: string
        onHand: string
        available: string
        adjustmentNo: string
        editVersion: number
        segregationNote: string
    } | null>(null)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [exportJob, setExportJob] = React.useState<InventoryExportJob | null>(
        null,
    )
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [pendingPayload, setPendingPayload] = React.useState<{
        stockAdjustmentId: string
        expectedBalanceLockVersion: number
        seedBalanceLockVersion: number
        reasonType: AdjustmentReasonType
        reasonTypeLabel: string
        direction: "increase" | "decrease"
        quantity: string
        note: string
        occurredAt: string
        idempotencyKey: string
        forceUnknown?: boolean
    } | null>(null)

    const rowFocusRef = React.useRef<Map<string, HTMLButtonElement | null>>(
        new Map(),
    )
    const restoreFocusIdRef = React.useRef<string | null>(null)
    const idempotencyRef = React.useRef<string | null>(null)

    // Debounce search → URL
    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    // `/` focuses list search (ignore when typing in inputs)
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl({ q: searchInput.trim() || null }, { replace: true })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl is stable enough via searchParams
    }, [searchInput])

    const query: InventoryQuery = React.useMemo(
        () => ({
            view,
            q: qParam || undefined,
            warehouseId,
            skuId,
            salesOrderLineId,
            availability,
            movementType,
            occurredFrom,
            occurredTo,
            cursor: cursorParam,
            pageSize: pagination.pageSize,
            sort: sortValue.split(",").filter(Boolean),
            balanceId: balanceIdParam,
            adjustmentId: adjustmentIdParam,
        }),
        [
            view,
            qParam,
            warehouseId,
            skuId,
            salesOrderLineId,
            availability,
            movementType,
            occurredFrom,
            occurredTo,
            cursorParam,
            pagination.pageSize,
            sortValue,
            balanceIdParam,
            adjustmentIdParam,
        ],
    )

    const listQuery = useInventoryListQuery(query)

    /** 深链筛选 chip 的业务名称（skuId/salesOrderLineId/adjustmentId 不直接上屏内部 ID）。 */
    const allViewRows = React.useMemo(
        () => [
            ...(listQuery.data?.balances ?? []),
            ...(listQuery.data?.movements ?? []),
            ...(listQuery.data?.reservations ?? []),
            ...(listQuery.data?.adjustments ?? []),
        ],
        [listQuery.data],
    )
    const detailQuery = useBalanceDetailQuery(previewBalanceId)
    const createDraftMutation = useCreateAdjustmentDraftMutation()
    const submitMutation = useSubmitAdjustmentMutation()
    const resolveUnknownMutation = useResolveAdjustmentUnknownMutation()
    const exportMutation = useStartInventoryExportMutation()

    const data = listQuery.data

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view, clearCursor: true },
            patch,
            options,
        )
    }

    const resetPagination = React.useCallback(() => {
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }, [])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPagination(next)
            const offset = next.pageIndex * next.pageSize
            patchUrl(
                {
                    cursor:
                        offset === 0
                            ? null
                            : encodeInventoryCursor(view, offset),
                    pageSize: String(next.pageSize),
                },
                { replace: true },
            )
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl uses the current URL snapshot
        [pathname, searchParams, view],
    )

    React.useEffect(() => {
        const expectedPageIndex = Math.floor(cursorOffset / pageSize)
        setPagination((current) =>
            current.pageIndex === expectedPageIndex &&
            current.pageSize === pageSize
                ? current
                : { pageIndex: expectedPageIndex, pageSize },
        )
    }, [cursorOffset, pageSize])

    // Restore focus after detail/adjust close
    React.useEffect(() => {
        if (previewBalanceId || adjustDraftId) return
        const id = restoreFocusIdRef.current
        if (!id) return
        const el = rowFocusRef.current.get(id)
        if (el) {
            el.focus()
            restoreFocusIdRef.current = null
        }
    }, [previewBalanceId, adjustDraftId])

    const form = useAppForm({
        defaultValues: {
            reasonType: "COUNT_LOSS" as AdjustmentReasonType,
            quantity: "",
            note: "",
            occurredAt: localNowInput(),
        },
        validators: {
            onChange: adjustSchema,
        },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const openDetail = React.useCallback(
        (balanceId: string) => {
            restoreFocusIdRef.current = balanceId
            setPreviewBalanceId(balanceId)
            // P2：打开详情属导航，用 push（不压缩历史）
            patchUrl({ balanceId })
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams, pathname, view],
    )

    const closeDetail = React.useCallback(() => {
        setPreviewBalanceId(null)
        // P2：关闭详情属导航，用 push
        patchUrl({ balanceId: null })
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl uses the current URL snapshot
    }, [searchParams, pathname, view])

    const startAdjustment = React.useCallback(
        async (row: StockBalanceRow) => {
            if (isPhoneNarrow) {
                setActionError(
                    "窄屏（移动端）仅支持只读查询；库存调整请在桌面完成。",
                )
                return
            }
            if (!row.allowedActions.includes("CREATE_ADJUSTMENT")) {
                setActionError(
                    row.actionBlockers.find(
                        (b) => b.action === "CREATE_ADJUSTMENT",
                    )?.message ?? "当前不允许发起库存调整",
                )
                return
            }
            restoreFocusIdRef.current = row.balanceId
            setActionError(null)
            setLastResult(null)
            idempotencyRef.current = null
            try {
                const draft = await createDraftMutation.mutateAsync({
                    balanceId: row.balanceId,
                })
                setAdjustBalanceId(row.balanceId)
                setAdjustDraftId(draft.stockAdjustmentId)
                setAdjustLockVersion(draft.balanceLockVersion)
                setAdjustSeedLock(row.lockVersion)
                setAdjustMeta({
                    warehouseName: draft.warehouseName,
                    skuCode: draft.skuCode,
                    skuName: draft.skuName,
                    baseUnit: draft.baseUnit,
                    onHand: row.onHandQuantity,
                    available: row.availableQuantity,
                    adjustmentNo: draft.adjustmentNo,
                    editVersion: draft.editVersion,
                    segregationNote: draft.segregationNote,
                })
                form.reset()
                form.setFieldValue("reasonType", draft.reasonType)
                form.setFieldValue("quantity", draft.quantity)
                form.setFieldValue("note", draft.note)
                form.setFieldValue(
                    "occurredAt",
                    draft.occurredAt.slice(0, 16) || localNowInput(),
                )
                setPreviewBalanceId(null)
            } catch (err) {
                setActionError(getErrorMessage(err, "创建调整草稿失败"))
            }
        },
        [createDraftMutation, form, isPhoneNarrow],
    )

    const closeAdjustment = React.useCallback(() => {
        setAdjustDraftId(null)
        setAdjustBalanceId(null)
        setAdjustMeta(null)
        setConfirmOpen(false)
        setPendingPayload(null)
    }, [])

    const doSubmit = React.useCallback(async () => {
        if (!adjustDraftId || !adjustMeta) return
        const values = form.state.values
        const reason =
            REASON_TYPE_OPTIONS.find((r) => r.value === values.reasonType) ??
            REASON_TYPE_OPTIONS[1]
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w10-adj-${adjustDraftId}-${Date.now()}`
        }
        const payload = {
            stockAdjustmentId: adjustDraftId,
            expectedBalanceLockVersion: adjustLockVersion,
            seedBalanceLockVersion: adjustSeedLock,
            reasonType: values.reasonType,
            reasonTypeLabel: reason.label,
            direction: reason.direction,
            quantity: values.quantity.trim(),
            note: values.note.trim(),
            occurredAt: values.occurredAt,
            idempotencyKey: idempotencyRef.current,
        }
        setPendingPayload(payload)
        setActionError(null)
        const result = await submitMutation.mutateAsync(payload)
        if (result.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "调整已提交待复核",
                description: `单号 ${result.outcome.adjustmentNo}。下一责任方：${result.outcome.nextResponsible}。余额尚未变化，确认入账后由系统刷新。`,
                reference: result.outcome.reference,
            })
            setConfirmOpen(false)
            closeAdjustment()
            return
        }
        if (result.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: resultText.unknown,
                description: result.message,
                reference: result.idempotencyKey,
                pendingIdempotencyKey: result.idempotencyKey,
            })
            setConfirmOpen(false)
            return
        }
        if (
            result.code === "VERSION_CONFLICT" &&
            result.latestLockVersion != null
        ) {
            setAdjustLockVersion(result.latestLockVersion)
            setActionError(result.message)
            setConfirmOpen(false)
            return
        }
        setActionError(result.message)
        setConfirmOpen(false)
    }, [
        adjustDraftId,
        adjustMeta,
        adjustLockVersion,
        adjustSeedLock,
        form.state.values,
        submitMutation,
        closeAdjustment,
    ])

    const {
        adjustmentColumns,
        balanceColumns,
        movementColumns,
        reservationColumns,
    } = useInventoryColumns({
        isPhoneNarrow,
        rowFocusRef,
        openDetail,
        startAdjustment,
    })
    if (listQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                    {Array.from({ length: 4 }).map((_, i) => (
                        <div
                            key={i}
                            className="h-20 animate-pulse rounded-lg bg-muted"
                        />
                    ))}
                </div>
                <div className="h-12 animate-pulse rounded-lg bg-muted" />
                <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (listQuery.isError || !data) {
        return (
            <PageScaffold>
                <PageHeader title="库存台账" description="加载失败" />
                <BusinessFailureState
                    error={listQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (data.emptyReason === "PERMISSION_REVOKED") {
        return (
            <PageScaffold>
                <PageHeader
                    title="库存台账"
                    description="模块权限已收回，相关数据已不再展示。"
                />
                <BusinessFailureState
                    kind="permission"
                    title="权限已收回"
                    description="当前账号的库存台账访问权限已被收回。余额、流水、导出结果与展开来源均不可见。"
                    action={
                        <Button
                            type="button"
                            onClick={() => void listQuery.refetch()}
                        >
                            重新检查权限
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (data.emptyReason === "NO_DATA_SCOPE") {
        return (
            <PageScaffold>
                <PageHeader
                    title="库存台账"
                    description="有模块权限但未配置仓库数据范围。"
                />
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置仓库数据范围"
                    description="不能显示为库存为 0。请联系管理员配置仓库授权后再查询。"
                />
            </PageScaffold>
        )
    }

    const pageRows = (() => {
        if (view === "balance") {
            return data.balances
        }
        if (view === "movement") {
            return data.movements
        }
        if (view === "reservation") {
            return data.reservations
        }
        return data.adjustments
    })()

    const chipSkuName = allViewRows.find((r) => r.skuId === skuId)?.skuName
    const chipSalesLineLabel = data.reservations.find(
        (r) => r.salesOrderLineId === salesOrderLineId,
    )?.salesOrderLineLabel
    const chipAdjustmentNo = data.adjustments.find(
        (a) => a.adjustmentId === adjustmentIdParam,
    )?.adjustmentNo

    const metricActive =
        availability === "zero"
            ? "zero"
            : availability === "reserved"
              ? "reserved"
              : view === "adjustment"
                ? "pending"
                : "combos"

    const detail = detailQuery.data

    return (
        <PageScaffold>
            <PageHeader
                title="库存台账"
                description={
                    isPhoneNarrow
                        ? "移动端只读：可查看余额与流水。库存调整、列设置与全量导出请在桌面完成。"
                        : "按仓库与 SKU 查看账面现存、有效预占与可用数量；追溯流水与销售预占。不可直接编辑库存或释放预占。"
                }
                breadcrumbs={[
                    { id: "proc", label: "采购与履约", href: "/inventory" },
                    { id: "inv", label: "库存台账", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={formatDateTime(
                            data.queriedAt,
                            "full",
                            "passthrough",
                        )}
                        dateTime={data.queriedAt}
                        state="fresh"
                        label="库存记录更新时间"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "refresh",
                                label: "刷新",
                                icon: RefreshCwIcon,
                                variant: "ghost",
                                onClick: () => {
                                    void listQuery.refetch()
                                    if (previewBalanceId)
                                        void detailQuery.refetch()
                                },
                            },
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled:
                                    !data.canExport ||
                                    data.total === 0 ||
                                    isPhoneNarrow,
                                onClick: () => {
                                    void exportMutation
                                        .mutateAsync({
                                            total: data.total,
                                            filterSummary: data.filterSummary,
                                        })
                                        .then((job) => setExportJob(job))
                                },
                            },
                        ]}
                    />
                }
            />

            {lastResult ? (
                <FormalActionResult
                    status={
                        lastResult.status === "succeeded"
                            ? "succeeded"
                            : lastResult.status === "unknown"
                              ? "unknown"
                              : "blocked"
                    }
                    title={lastResult.title}
                    description={lastResult.description}
                    reference={lastResult.reference}
                    referenceLabel={
                        lastResult.status === "unknown" ? "原任务号" : undefined
                    }
                    actions={
                        lastResult.pendingIdempotencyKey ? (
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={resolveUnknownMutation.isPending}
                                    onClick={() => {
                                        void resolveUnknownMutation
                                            .mutateAsync({
                                                idempotencyKey:
                                                    lastResult.pendingIdempotencyKey!,
                                                stockAdjustmentId:
                                                    pendingPayload?.stockAdjustmentId,
                                                expectedBalanceLockVersion:
                                                    pendingPayload?.expectedBalanceLockVersion,
                                            })
                                            .then((r) => {
                                                if (r.status === "succeeded") {
                                                    setLastResult({
                                                        status: "succeeded",
                                                        title: "调整已提交待复核",
                                                        description: `单号 ${r.outcome.adjustmentNo}。下一责任方：${r.outcome.nextResponsible}。`,
                                                        reference:
                                                            r.outcome.reference,
                                                    })
                                                    closeAdjustment()
                                                } else if (
                                                    r.status === "unknown"
                                                ) {
                                                    setLastResult({
                                                        status: "unknown",
                                                        title: "仍在查询最终结果",
                                                        description: r.message,
                                                        reference:
                                                            r.idempotencyKey,
                                                        pendingIdempotencyKey:
                                                            r.idempotencyKey,
                                                    })
                                                } else {
                                                    setActionError(r.message)
                                                }
                                            })
                                    }}
                                >
                                    查询最终结果
                                </Button>
                            </div>
                        ) : undefined
                    }
                />
            ) : null}

            {exportJob ? (
                <BackgroundJobProgress
                    mode="all-or-nothing"
                    status={
                        exportJob.status === "queued"
                            ? "queued"
                            : exportJob.status === "running"
                              ? "running"
                              : exportJob.status === "succeeded"
                                ? "succeeded"
                                : "failed"
                    }
                    total={exportJob.total}
                    completed={exportJob.completed}
                    succeeded={
                        exportJob.status === "succeeded"
                            ? exportJob.total
                            : undefined
                    }
                    label="库存台账导出"
                    description={
                        <>
                            范围：{exportJob.filterSummary}
                            。导出文件由系统生成，完成后可在此下载。
                            {exportJob.downloadLabel ? (
                                <span className="mt-1 block font-medium">
                                    可下载：{exportJob.downloadLabel}
                                </span>
                            ) : null}
                        </>
                    }
                    action={
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => setExportJob(null)}
                        >
                            关闭
                        </Button>
                    }
                />
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未完成</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            <details className={`${surfaceInsetClassName} px-3 py-2.5 text-sm`}>
                <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                    自有实物库存边界说明
                </summary>
                <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
                    {data.excludedKindsNote}
                    <span className="mt-1 block">{data.openingStockNote}</span>
                </p>
            </details>

            <MetricStrip columns={4} aria-label="库存台账指标筛选">
                {/* 指标 = view + availability 组合语义（视图快捷组合，有业务价值）：点击同时写
            view 与 availability 两个参数；与工具栏「可用状态」下拉共享 availability 参数
            天然同步；Tabs（view）与指标条同源 URL，保持一致。 */}
                <MetricFilterItem
                    label="库存组合"
                    value={data.metrics.balanceDimensionCount}
                    detail="仓库+SKU 组合数"
                    active={metricActive === "combos" && view === "balance"}
                    onClick={() => {
                        patchUrl(
                            {
                                view: "balance",
                                availability: "all",
                            },
                            { replace: true },
                        )
                        resetPagination()
                    }}
                />
                <MetricFilterItem
                    label="有效预占组合"
                    value={data.metrics.reservedDimensionCount}
                    detail="有有效预占"
                    active={metricActive === "reserved"}
                    onClick={() => {
                        patchUrl(
                            {
                                view: "balance",
                                availability: "reserved",
                            },
                            { replace: true },
                        )
                        resetPagination()
                    }}
                />
                <MetricFilterItem
                    label="零可用组合"
                    value={data.metrics.zeroAvailableDimensionCount}
                    detail="可用数量为 0"
                    active={metricActive === "zero"}
                    onClick={() => {
                        patchUrl(
                            {
                                view: "balance",
                                availability: "zero",
                            },
                            { replace: true },
                        )
                        resetPagination()
                    }}
                />
                <MetricFilterItem
                    label="待处理调整"
                    value={data.metrics.pendingAdjustmentCount}
                    detail="处理中"
                    active={metricActive === "pending"}
                    onClick={() => {
                        patchUrl(
                            {
                                view: "adjustment",
                                availability: null,
                            },
                            { replace: true },
                        )
                        resetPagination()
                    }}
                />
            </MetricStrip>

            <Tabs
                value={view}
                onValueChange={(v) => {
                    const nextView = v as InventoryView
                    // 排序参数跨视图残留会让下拉显示占位而旧排序仍生效：不属于目标视图则一并清掉。
                    const validSorts = sortOptions(nextView).map((o) => o.value)
                    const patch: Record<string, string | null | undefined> = {
                        view: nextView,
                    }
                    if (sortValue && !validSorts.includes(sortValue)) {
                        patch.sort = null
                    }
                    patchUrl(patch, { replace: true })
                    resetPagination()
                }}
            >
                <TabsList variant="line" className="w-full justify-start">
                    {(
                        [
                            "balance",
                            "movement",
                            "reservation",
                            "adjustment",
                        ] as const
                    ).map((v) => (
                        <TabsTrigger key={v} value={v}>
                            {VIEW_LABEL[v]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <BusinessTableFrame
                title={VIEW_LABEL[view]}
                description={
                    <span aria-live="polite">
                        {data.filterSummary}
                        {view === "balance" ? (
                            <span className="text-muted-foreground">
                                {" "}
                                · 数量均带基础单位；可用数量以系统数据为准
                            </span>
                        ) : null}
                    </span>
                }
                toolbar={
                    <ListToolbar
                        search={
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    ref={searchInputRef}
                                    value={searchInput}
                                    onChange={(e) => {
                                        setSearchInput(e.target.value)
                                        resetPagination()
                                    }}
                                    placeholder="SKU 编码、名称、规格、仓库"
                                    aria-label="搜索库存"
                                />
                            </InputGroup>
                        }
                        filters={
                            <>
                                <label className="flex items-center gap-1.5 text-sm">
                                    <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                        仓库
                                    </span>
                                    <WarehouseSearchCombobox
                                        className="w-44"
                                        value={warehouseId || undefined}
                                        onValueChange={(id) => {
                                            patchUrl(
                                                {
                                                    warehouseId: id || null,
                                                },
                                                { replace: true },
                                            )
                                            resetPagination()
                                        }}
                                        purpose="filter"
                                        aria-label="筛选仓库"
                                        placeholder="全部仓库"
                                    />
                                </label>
                                {view === "balance" ? (
                                    <label className="flex items-center gap-1.5 text-sm">
                                        <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                            可用状态
                                        </span>
                                        <OptionCombobox
                                            className="w-28"
                                            value={availability}
                                            onValueChange={(v) => {
                                                patchUrl(
                                                    {
                                                        availability: (v ??
                                                            "all") as InventoryAvailability,
                                                    },
                                                    { replace: true },
                                                )
                                                resetPagination()
                                            }}
                                            options={(
                                                [
                                                    "all",
                                                    "positive",
                                                    "zero",
                                                    "reserved",
                                                ] as const
                                            ).map((a) => ({
                                                value: a,
                                                label: AVAILABILITY_LABEL[a],
                                            }))}
                                            size="sm"
                                            allowClear={false}
                                            aria-label="筛选可用状态"
                                            placeholder="可用状态"
                                        />
                                    </label>
                                ) : null}
                                {view === "movement" ? (
                                    <>
                                        <label className="flex items-center gap-1.5 text-sm">
                                            <span className="sr-only">
                                                流水类型
                                            </span>
                                            <OptionCombobox
                                                className="w-32"
                                                value={movementType[0] ?? "all"}
                                                onValueChange={(value) => {
                                                    patchUrl(
                                                        {
                                                            movementType:
                                                                value &&
                                                                value !== "all"
                                                                    ? value
                                                                    : null,
                                                        },
                                                        { replace: true },
                                                    )
                                                    resetPagination()
                                                }}
                                                options={[
                                                    {
                                                        value: "all",
                                                        label: "全部流水",
                                                    },
                                                    ...MOVEMENT_TYPE_OPTIONS,
                                                ]}
                                                size="sm"
                                                allowClear={false}
                                                aria-label="筛选流水类型"
                                                placeholder="全部流水"
                                            />
                                        </label>
                                        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                                            发生日期
                                            <Input
                                                type="date"
                                                className="h-8 w-32"
                                                value={occurredFrom ?? ""}
                                                max={occurredTo}
                                                onChange={(event) => {
                                                    patchUrl(
                                                        {
                                                            occurredFrom:
                                                                event.target
                                                                    .value,
                                                        },
                                                        { replace: true },
                                                    )
                                                    resetPagination()
                                                }}
                                                aria-label="发生日期起"
                                            />
                                            <span>至</span>
                                            <Input
                                                type="date"
                                                className="h-8 w-32"
                                                value={occurredTo ?? ""}
                                                min={occurredFrom}
                                                onChange={(event) => {
                                                    patchUrl(
                                                        {
                                                            occurredTo:
                                                                event.target
                                                                    .value,
                                                        },
                                                        { replace: true },
                                                    )
                                                    resetPagination()
                                                }}
                                                aria-label="发生日期止"
                                            />
                                        </label>
                                    </>
                                ) : null}
                            </>
                        }
                        secondary={
                            skuId || salesOrderLineId || adjustmentIdParam ? (
                                <>
                                    {skuId ? (
                                        <ChipFilter
                                            label={`当前 SKU：${chipSkuName ?? "已定位单品"}`}
                                            onClear={() => {
                                                patchUrl(
                                                    { skuId: null },
                                                    { replace: true },
                                                )
                                                resetPagination()
                                            }}
                                        />
                                    ) : null}
                                    {salesOrderLineId ? (
                                        <ChipFilter
                                            label={`销售单明细：${chipSalesLineLabel ?? "已定位"}`}
                                            onClear={() => {
                                                patchUrl(
                                                    { salesOrderLineId: null },
                                                    { replace: true },
                                                )
                                                resetPagination()
                                            }}
                                        />
                                    ) : null}
                                    {adjustmentIdParam ? (
                                        <ChipFilter
                                            label={`调整单：${chipAdjustmentNo ?? "已定位"}`}
                                            onClear={() => {
                                                patchUrl(
                                                    { adjustmentId: null },
                                                    { replace: true },
                                                )
                                                resetPagination()
                                            }}
                                        />
                                    ) : null}
                                </>
                            ) : undefined
                        }
                        actions={
                            <>
                                <label className="flex items-center gap-1.5 text-sm">
                                    <span className="sr-only">排序</span>
                                    <OptionCombobox
                                        className="w-40"
                                        value={sortValue}
                                        onValueChange={(value) => {
                                            patchUrl(
                                                {
                                                    sort:
                                                        value ??
                                                        defaultSortValue(view),
                                                },
                                                { replace: true },
                                            )
                                            resetPagination()
                                        }}
                                        options={sortOptions(view)}
                                        size="sm"
                                        allowClear={false}
                                        aria-label="排序方式"
                                        placeholder="排序"
                                    />
                                </label>
                                <span
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    共 {data.total.toLocaleString("zh-CN")} 条
                                </span>
                                {(qParam ||
                                    warehouseId ||
                                    (availability !== "all" &&
                                        view === "balance") ||
                                    skuId ||
                                    salesOrderLineId ||
                                    adjustmentIdParam ||
                                    movementType.length > 0 ||
                                    searchParams.has("occurredFrom") ||
                                    searchParams.has("occurredTo")) && (
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => {
                                            setSearchInput("")
                                            // P4：清全部筛选参数；保留视图、排序与预览（balanceId 导航上下文）
                                            patchUrl(
                                                {
                                                    q: null,
                                                    warehouseId: null,
                                                    availability: "all",
                                                    skuId: null,
                                                    salesOrderLineId: null,
                                                    adjustmentId: null,
                                                    movementType: null,
                                                    occurredFrom: null,
                                                    occurredTo: null,
                                                },
                                                { replace: true },
                                            )
                                            resetPagination()
                                        }}
                                    >
                                        清除筛选
                                    </Button>
                                )}
                            </>
                        }
                    />
                }
                table={
                    data.total === 0 ? (
                        data.emptyReason === "FILTER_NO_RESULT" ? (
                            <BusinessEmptyState
                                kind="filter"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="当前筛选无结果"
                                description={`没有符合「${data.filterSummary}」的记录。可清除筛选或切换视图。`}
                                action={
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        className="rounded-lg shadow-none"
                                        onClick={() => {
                                            setSearchInput("")
                                            // P4：清全部筛选参数；保留当前视图（不强制回 balance）
                                            patchUrl(
                                                {
                                                    q: null,
                                                    warehouseId: null,
                                                    availability: "all",
                                                    skuId: null,
                                                    salesOrderLineId: null,
                                                    adjustmentId: null,
                                                },
                                                { replace: true },
                                            )
                                        }}
                                    >
                                        清除筛选
                                    </Button>
                                }
                            />
                        ) : (
                            <BusinessEmptyState
                                kind="no-data"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="当前仓库尚无 ERP 自有库存记录"
                                description="期初库存需在「导入与期初」完成导入后才会形成流水；商城旧库存不会自动显示在此。"
                                action={
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        className="rounded-lg shadow-none"
                                        render={
                                            <Link href="/governance/imports" />
                                        }
                                    >
                                        前往导入与期初
                                    </Button>
                                }
                            />
                        )
                    ) : view === "balance" ? (
                        <DataTable
                            data={pageRows as StockBalanceRow[]}
                            loading={
                                listQuery.isFetching && !listQuery.isPending
                            }
                            showRefreshingBanner={listQuery.isFetching}
                            columns={balanceColumns}
                            getRowId={(row) => row.balanceId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            defaultColumnPinning={{
                                left: ["identity"],
                                right: ["actions"],
                            }}
                            onRowPreview={(row) => openDetail(row.balanceId)}
                            onRowOpen={(row) => openDetail(row.balanceId)}
                        />
                    ) : view === "movement" ? (
                        <DataTable
                            data={pageRows as StockMovementRow[]}
                            loading={
                                listQuery.isFetching && !listQuery.isPending
                            }
                            showRefreshingBanner={listQuery.isFetching}
                            columns={movementColumns}
                            getRowId={(row) => row.movementId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            defaultColumnPinning={{ left: ["identity"] }}
                        />
                    ) : view === "reservation" ? (
                        <DataTable
                            data={pageRows as StockReservationRow[]}
                            loading={
                                listQuery.isFetching && !listQuery.isPending
                            }
                            showRefreshingBanner={listQuery.isFetching}
                            columns={reservationColumns}
                            getRowId={(row) => row.reservationId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            defaultColumnPinning={{
                                left: ["identity"],
                                right: ["actions"],
                            }}
                        />
                    ) : (
                        <DataTable
                            data={pageRows as StockAdjustmentRow[]}
                            loading={
                                listQuery.isFetching && !listQuery.isPending
                            }
                            showRefreshingBanner={listQuery.isFetching}
                            columns={adjustmentColumns}
                            getRowId={(row) => row.adjustmentId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            defaultColumnPinning={{ left: ["doc"] }}
                        />
                    )
                }
            />

            <InventoryBalancePreview
                open={previewBalanceId != null}
                detail={detail}
                isPending={detailQuery.isPending}
                onClose={closeDetail}
                onViewMovements={(currentDetail) => {
                    setPreviewBalanceId(null)
                    patchUrl(
                        {
                            view: "movement",
                            balanceId: null,
                            warehouseId: currentDetail.balance.warehouseId,
                            skuId: currentDetail.balance.skuId,
                        },
                        { replace: true },
                    )
                    resetPagination()
                }}
                onStartAdjustment={startAdjustment}
            />
            <Dialog
                open={adjustDraftId != null}
                onOpenChange={(open) => {
                    if (!open) closeAdjustment()
                }}
            >
                <DialogContent className="sm:max-w-lg">
                    <DialogHeader>
                        <DialogTitle>发起库存调整</DialogTitle>
                        <DialogDescription>
                            从当前余额上下文创建调整单草稿。提交后进入仓储复核，不会立即改库存。
                        </DialogDescription>
                    </DialogHeader>

                    {adjustMeta ? (
                        <div className="space-y-4">
                            <div className="rounded-xl border bg-muted/40 p-3 text-sm">
                                <div className="font-medium">
                                    {adjustMeta.warehouseName}
                                    <span className="num ml-2 text-muted-foreground">
                                        {adjustMeta.skuCode}
                                    </span>
                                </div>
                                <div>{adjustMeta.skuName}</div>
                                <div className="mt-2 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                                    <div>
                                        账面现存{" "}
                                        <span className="num text-foreground">
                                            {adjustMeta.onHand}{" "}
                                            {adjustMeta.baseUnit}
                                        </span>
                                    </div>
                                    <div>
                                        可用{" "}
                                        <span className="num text-foreground">
                                            {adjustMeta.available}{" "}
                                            {adjustMeta.baseUnit}
                                        </span>
                                    </div>
                                    <div>
                                        草稿号{" "}
                                        <span className="num text-foreground">
                                            {adjustMeta.adjustmentNo}
                                        </span>
                                    </div>
                                    <div>
                                        数据版本{" "}
                                        <span className="num text-foreground">
                                            已按最新核对
                                        </span>
                                    </div>
                                </div>
                            </div>

                            <Alert>
                                <SlashIcon className="size-4" aria-hidden />
                                <AlertTitle>岗位分离</AlertTitle>
                                <AlertDescription className="text-xs">
                                    {adjustMeta.segregationNote}
                                </AlertDescription>
                            </Alert>

                            <form
                                className="space-y-3"
                                onSubmit={(e) => {
                                    e.preventDefault()
                                    void form.handleSubmit()
                                }}
                            >
                                <div className="grid gap-1.5">
                                    <Label htmlFor="reasonType">原因类型</Label>
                                    <form.AppField
                                        name="reasonType"
                                        children={(field) => (
                                            <OptionCombobox
                                                id="reasonType"
                                                value={field.state.value}
                                                onValueChange={(v) => {
                                                    field.handleChange(
                                                        (v ??
                                                            field.state
                                                                .value) as AdjustmentReasonType,
                                                    )
                                                }}
                                                options={REASON_TYPE_OPTIONS.map(
                                                    (opt) => ({
                                                        value: opt.value,
                                                        label: `${opt.label}（${
                                                            opt.direction ===
                                                            "increase"
                                                                ? "增加"
                                                                : "减少"
                                                        }）`,
                                                    }),
                                                )}
                                                className="w-full"
                                                allowClear={false}
                                                aria-label="原因类型"
                                                placeholder="原因类型"
                                            />
                                        )}
                                    />
                                </div>

                                <form.AppField
                                    name="quantity"
                                    children={(field) => (
                                        <field.TextField
                                            label={`调整数量（${adjustMeta.baseUnit}，正数）`}
                                        />
                                    )}
                                />

                                <form.AppField
                                    name="occurredAt"
                                    children={(field) => (
                                        <div className="space-y-1.5">
                                            <Label htmlFor="adjust-occured-at">
                                                业务发生时间
                                            </Label>
                                            <DateTimeLocalPicker
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(next) =>
                                                    field.handleChange(
                                                        next ?? "",
                                                    )
                                                }
                                                className="w-full"
                                            />
                                            {field.state.meta.errors[0] ? (
                                                <p
                                                    className="text-xs text-destructive"
                                                    role="alert"
                                                >
                                                    {String(
                                                        field.state.meta
                                                            .errors[0],
                                                    )}
                                                </p>
                                            ) : null}
                                        </div>
                                    )}
                                />

                                <form.AppField
                                    name="note"
                                    children={(field) => (
                                        <field.TextareaField label="原因说明" />
                                    )}
                                />

                                <div className="rounded-lg border bg-card p-3 text-xs text-muted-foreground space-y-1">
                                    <div className="font-medium text-foreground">
                                        提交约束
                                    </div>
                                    <ul className="list-disc pl-4 space-y-0.5">
                                        <li>不会直接修改账面或可用数量</li>
                                        <li>
                                            经办与复核岗位分离，提交后待仓储复核
                                        </li>
                                        <li>
                                            按当前数据版本提交；若已被他人修改，将提示冲突并保留你的输入。
                                        </li>
                                    </ul>
                                </div>

                                <DialogFooter className="gap-2 sm:justify-between">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        onClick={closeAdjustment}
                                    >
                                        取消
                                    </Button>
                                    <form.AppForm>
                                        <form.SubmitButton label="提交待复核" />
                                    </form.AppForm>
                                </DialogFooter>
                            </form>
                        </div>
                    ) : null}
                </DialogContent>
            </Dialog>

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                actionLabel="提交库存调整"
                confirmLabel="确认提交"
                fromStatus={{ label: "草稿", tone: "neutral" }}
                toStatus={{ label: "待仓储复核", tone: "warning" }}
                description="确认后形成调整单，进入仓储复核队列。余额在确认入账前不会变化。"
                lockedFields={[
                    adjustMeta
                        ? `${adjustMeta.warehouseName} / ${adjustMeta.skuCode}`
                        : "当前余额",
                    "已按当前数据版本核对",
                ]}
                effects={[
                    "创建待仓储复核的库存调整单",
                    "不立即修改账面、预占和可用数量",
                    "经办人不得自行复核或确认入账",
                ]}
                nextDepartment="仓储复核"
                irreversibleEffects={["形成调整单号并进入连续队列"]}
                pending={submitMutation.isPending}
                onConfirm={() => void doSubmit()}
            />
        </PageScaffold>
    )
}
