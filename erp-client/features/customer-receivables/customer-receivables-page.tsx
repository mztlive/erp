"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"
import {
    DownloadIcon,
    FileTextIcon,
    RefreshCwIcon,
    SearchIcon,
    WalletIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FilterChip,
    FormalActionResult,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    MoneyValue,
    OptionCombobox,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { AllocationSessionPanel } from "@/features/customer-receivables/allocation-session-panel"
import {
    createInvoiceColumns,
    createReceivableColumns,
    createReceiptColumns,
    type CustomerAccountPreviewTarget,
} from "@/features/customer-receivables/customer-account-columns"
import {
    CustomerAccountDetailPreview,
    type ReverseRequest,
} from "@/features/customer-receivables/customer-account-detail-preview"
import {
    buildAccountsCsv,
    downloadCsv,
} from "@/features/customer-receivables/export-csv"
import { ReceivableActionDialogs } from "@/features/customer-receivables/receivable-action-dialogs"
import {
    useAllocationSessionQuery,
    useCreateAllocationSessionMutation,
    useCustomerAccountsDetailQuery,
    useCustomerAccountsListQuery,
    useReverseFactMutation,
} from "@/features/customer-receivables/queries"
import type {
    AllocationMode,
    CustomerAccountsQuery,
    CustomerAccountsView,
    DueFilter,
} from "@/features/customer-receivables/types"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/receivable-counterparty-search-combobox"
import { DUE_LABEL, VIEW_LABEL } from "@/features/customer-receivables/types"
import { freshnessText } from "@/lib/ui-text"

function parseView(raw: string | null): CustomerAccountsView {
    if (
        raw === "receipt" ||
        raw === "sales_invoice" ||
        raw === "unallocated" ||
        raw === "receivable"
    ) {
        return raw
    }
    return "receivable"
}

function parseDue(raw: string | null): DueFilter | undefined {
    if (
        raw === "not_due" ||
        raw === "due_today" ||
        raw === "overdue" ||
        raw === "all"
    ) {
        return raw
    }
    return undefined
}

export function CustomerReceivablesPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const counterpartyPartyId = searchParams.get("counterpartyId") ?? undefined
    const customerId = searchParams.get("customerId") ?? undefined
    const due = parseDue(searchParams.get("due"))
    const status = searchParams.get("status") ?? undefined
    const reviewStatus = searchParams.get("reviewStatus") ?? undefined
    const focusId = searchParams.get("focusId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const receivableAccountId =
        searchParams.get("receivableAccountId") ?? undefined
    const returnTo = searchParams.get("returnTo") ?? undefined
    const from = searchParams.get("from") ?? undefined
    const sessionId = searchParams.get("sessionId") ?? undefined
    const previewKind = searchParams.get("previewKind") as
        | CustomerAccountPreviewTarget["kind"]
        | null
    const previewId = searchParams.get("previewId") ?? undefined

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [preview, setPreview] =
        React.useState<CustomerAccountPreviewTarget | null>(() =>
            previewKind && previewId
                ? { kind: previewKind, id: previewId }
                : focusId
                  ? { kind: "receivable", id: focusId }
                  : null,
        )
    const [partyPickerOpen, setPartyPickerOpen] = React.useState(false)
    const [partyPickerMode, setPartyPickerMode] =
        React.useState<AllocationMode>("receipt")
    const [selectedPartyId, setSelectedPartyId] = React.useState("")
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [reverseConfirm, setReverseConfirm] = React.useState<{
        kind: "receipt_reverse" | "refund" | "red_invoice"
        sourceFactId: string
        label: string
        amount?: string
    } | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [reverseAmount, setReverseAmount] = React.useState("")

    const query: CustomerAccountsQuery = React.useMemo(
        () => ({
            view,
            q: qParam || undefined,
            counterpartyPartyId,
            customerId,
            due,
            status,
            reviewStatus,
            salesOrderId,
            receivableAccountId,
            returnTo,
            from,
        }),
        [
            view,
            qParam,
            counterpartyPartyId,
            customerId,
            due,
            status,
            reviewStatus,
            salesOrderId,
            receivableAccountId,
            returnTo,
            from,
        ],
    )

    // 分页从 URL 派生（P6）；筛选变更写 URL 并回第 1 页。
    const pageFromUrl = React.useMemo(
        () =>
            Math.max(
                1,
                Number.parseInt(searchParams.get("page") ?? "1", 10) || 1,
            ),
        [searchParams],
    )
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, pageFromUrl - 1), pageSize: 20 }),
        [pageFromUrl],
    )

    const listQuery = useCustomerAccountsListQuery(query)
    const detailQuery = useCustomerAccountsDetailQuery(
        preview?.kind ?? null,
        preview?.id ?? null,
    )
    const sessionQuery = useAllocationSessionQuery(sessionId ?? null)
    const createSession = useCreateAllocationSessionMutation()
    const reverseMutation = useReverseFactMutation()

    const data = listQuery.data

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    /** 客户锁定（customerId）显性化为可移除 chip。 */
    const lockedCustomerName = React.useMemo(
        () =>
            (data?.counterparties ?? []).find(
                (c) => c.customerId === customerId,
            )?.customerName,
        [data?.counterparties, customerId],
    )

    const hasActiveFilters = Boolean(
        qParam.trim() ||
        counterpartyPartyId ||
        customerId ||
        due ||
        status ||
        reviewStatus ||
        salesOrderId ||
        receivableAccountId,
    )

    /** P4：清全部筛选参数 + 分页回 1；保留 view/导航上下文。 */
    const clearFilters = React.useCallback(() => {
        setSearchInput("")
        patchUrl(
            {
                q: null,
                counterpartyId: null,
                customerId: null,
                due: null,
                status: null,
                reviewStatus: null,
                salesOrderId: null,
                receivableAccountId: null,
                focusId: null,
                previewKind: null,
                previewId: null,
                page: null,
            },
            { replace: true },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchParams, pathname, view])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            patchUrl(
                {
                    page:
                        next.pageIndex + 1 > 1
                            ? String(next.pageIndex + 1)
                            : null,
                },
                { replace: true },
            )
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams, pathname, view],
    )

    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl(
                { q: searchInput.trim() || null, page: null },
                { replace: true },
            )
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchInput])

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

    // W05 链入：自动打开核销会话（有 salesOrderId + counterparty 或可推断）
    const autoSessionRef = React.useRef(false)
    React.useEffect(() => {
        if (autoSessionRef.current || sessionId || !data) return
        if (from !== "W05" || !returnTo) return
        if (!data.canRegister) return
        const party =
            counterpartyPartyId ??
            data.receivables[0]?.counterpartyPartyId ??
            data.counterparties.find((c) => c.customerId === customerId)
                ?.counterpartyPartyId
        if (!party) return
        autoSessionRef.current = true
        void (async () => {
            try {
                const session = await createSession.mutateAsync({
                    mode: "receipt",
                    counterpartyPartyId: party,
                    salesOrderId,
                    receivableAccountId,
                    returnTo,
                    from,
                })
                patchUrl(
                    { sessionId: session.draftSessionId },
                    { replace: true },
                )
            } catch (err) {
                setActionError(getErrorMessage(err, "无法开始本次核销"))
            }
        })()
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [data, from, returnTo, sessionId])

    const openPreview = React.useCallback(
        (next: CustomerAccountPreviewTarget | null) => {
            setPreview(next)
            if (next) {
                // 打开/关闭详情用 push（P2）；旧 focusId 一并清理
                patchUrl(
                    {
                        previewKind: next.kind,
                        previewId: next.id,
                        focusId: null,
                    },
                    { replace: false },
                )
            }
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams, pathname, view],
    )

    const closePreview = React.useCallback(() => {
        setPreview(null)
        patchUrl(
            {
                previewKind: null,
                previewId: null,
                focusId: null,
            },
            { replace: false },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchParams, pathname, view])

    async function startSession(
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: { salesOrderId?: string; receivableAccountId?: string },
    ) {
        setActionError(null)
        setLastResult(null)
        try {
            const session = await createSession.mutateAsync({
                mode,
                counterpartyPartyId: partyId,
                existingFactId,
                salesOrderId: target?.salesOrderId ?? salesOrderId,
                receivableAccountId:
                    target?.receivableAccountId ?? receivableAccountId,
                returnTo,
                from,
            })
            setPartyPickerOpen(false)
            patchUrl({
                sessionId: session.draftSessionId,
                counterpartyId: partyId,
            })
        } catch (err) {
            setActionError(getErrorMessage(err, "创建本次核销失败"))
        }
    }

    function openRegister(mode: AllocationMode) {
        setPartyPickerMode(mode)
        setSelectedPartyId(counterpartyPartyId ?? "")
        setPartyPickerOpen(true)
    }

    async function confirmReverse() {
        if (!reverseConfirm) return
        const key = `w11-rev-${reverseConfirm.sourceFactId}-${Date.now()}`
        const res = await reverseMutation.mutateAsync({
            kind: reverseConfirm.kind,
            sourceFactId: reverseConfirm.sourceFactId,
            amount:
                reverseConfirm.kind === "red_invoice"
                    ? reverseAmount
                    : undefined,
            reason: reverseReason || "纠错",
            idempotencyKey: key,
        })
        if (res.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "反向记录已追加",
                description: res.message,
                reference: res.operationId,
                facts: [
                    { label: "反向单号", value: res.reverseFactNo },
                    { label: "原记录", value: reverseConfirm.label },
                ],
            })
            setReverseConfirm(null)
            setReverseReason("")
            setReverseAmount("")
            closePreview()
            return
        }
        if (res.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "纠错结果不确定",
                description: res.message,
                reference: res.idempotencyKey,
            })
            setReverseConfirm(null)
            return
        }
        setActionError(res.message)
        setReverseConfirm(null)
    }

    const receivableColumns = React.useMemo(
        () =>
            createReceivableColumns({
                onPreview: openPreview,
                onStartSession: startSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.canRegister],
    )

    const receiptColumns = React.useMemo(
        () =>
            createReceiptColumns({
                onPreview: openPreview,
                onStartSession: startSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [],
    )

    const invoiceColumns = React.useMemo(
        () =>
            createInvoiceColumns({
                onPreview: openPreview,
                onStartSession: startSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [],
    )
    // 核销会话全屏
    if (sessionId) {
        if (sessionQuery.isPending) {
            return (
                <PageScaffold>
                    <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            )
        }
        if (!sessionQuery.data) {
            return (
                <PageScaffold>
                    <BusinessFailureState
                        kind="business"
                        title="本次核销无效"
                        description="本次核销已失效，请重新开始。"
                        action={
                            <Button
                                type="button"
                                onClick={() => patchUrl({ sessionId: null })}
                            >
                                返回列表
                            </Button>
                        }
                    />
                </PageScaffold>
            )
        }
        return (
            <PageScaffold>
                <AllocationSessionPanel
                    session={sessionQuery.data}
                    onClose={() => {
                        const ret = sessionQuery.data?.returnContext
                        if (ret?.returnTo && ret.from === "W05") {
                            router.push(ret.returnTo)
                            return
                        }
                        patchUrl({ sessionId: null })
                    }}
                    onPosted={() => {
                        void listQuery.refetch()
                    }}
                />
            </PageScaffold>
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="客户往来加载失败"
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

    const metrics = data?.metrics
    return (
        <PageScaffold density="compact">
            <PageHeader
                title="客户往来"
                breadcrumbs={[
                    {
                        id: "fin",
                        label: "财务",
                        href: "/finance/customer-accounts",
                    },
                    { id: "ar", label: "客户往来", current: true },
                ]}
                metadata={
                    data ? (
                        <DataFreshness
                            updatedAt={freshnessText.dataUpdatedAt}
                            dateTime={data.queriedAt}
                            state="fresh"
                            label="客户往来"
                        />
                    ) : null
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled: !data?.canExport || data.total === 0,
                                onClick: () => {
                                    if (!data) return
                                    const fileName = `客户往来-${VIEW_LABEL[data.view]}-${new Date().toISOString().slice(0, 10)}.csv`
                                    downloadCsv(
                                        fileName,
                                        buildAccountsCsv(data),
                                    )
                                    setLastResult({
                                        status: "succeeded",
                                        title: "导出已完成",
                                        description: `已按当前筛选生成 CSV 文件 ${fileName}，并开始下载。`,
                                    })
                                },
                            },
                            {
                                actionKey: "register-invoice",
                                label: "登记销项发票",
                                icon: FileTextIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled: !data?.canRegister,
                                title: data?.canRegister
                                    ? undefined
                                    : "当前无销项发票登记权限",
                                onClick: () => openRegister("invoice"),
                            },
                            {
                                actionKey: "register-receipt",
                                label: "登记回款",
                                icon: WalletIcon,
                                mobileVisibility: "hide",
                                disabled: !data?.canRegister,
                                title: data?.canRegister
                                    ? undefined
                                    : "当前无回款登记权限",
                                onClick: () => openRegister("receipt"),
                            },
                        ]}
                    />
                }
            />

            {from === "W05" && returnTo ? (
                <Alert variant="info">
                    <AlertTitle>销售单票款入口</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-2">
                        已携带来源页签返回上下文
                        {salesOrderId
                            ? ` · 销售单 ${
                                  data?.receivables.find(
                                      (r) => r.salesOrderId === salesOrderId,
                                  )?.salesOrderNo ?? ""
                              }`
                            : ""}
                        。核销完成后可回到销售单。
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href={returnTo} />}
                        >
                            返回销售单
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}

            {lastResult ? (
                <FormalActionResult
                    status={
                        lastResult.status === "failed"
                            ? "blocked"
                            : lastResult.status
                    }
                    title={lastResult.title}
                    description={lastResult.description}
                    reference={lastResult.reference}
                    facts={lastResult.facts}
                />
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未成功</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {data && !data.moduleAllowed ? (
                <BusinessFailureState
                    kind="permission"
                    description="无客户往来模块权限或权限已收回。"
                />
            ) : data && !data.hasDataScope ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置客户往来范围"
                    description="不得用 0 元假装无应收。请申请财务数据范围。"
                />
            ) : (
                <>
                    {metrics ? (
                        <MetricStrip columns={4} aria-label="客户往来指标">
                            <MetricFilterItem
                                label="开放应收"
                                value={
                                    <MoneyValue
                                        value={metrics.openReceivableTotal}
                                    />
                                }
                                detail={
                                    data
                                        ? `更新 ${formatDateTime(data.queriedAt, "monthDayIntl")}`
                                        : undefined
                                }
                                active={view === "receivable"}
                                onClick={() => {
                                    // 其余指标点击只设 view（P7），回第 1 页
                                    patchUrl(
                                        { view: "receivable", page: null },
                                        { replace: true },
                                    )
                                }}
                            />
                            <MetricFilterItem
                                label="已逾期应收"
                                value={
                                    <MoneyValue
                                        value={metrics.overdueReceivableTotal}
                                    />
                                }
                                detail="需催收"
                                active={
                                    view === "receivable" && due === "overdue"
                                }
                                onClick={() => {
                                    // view+filter 双重语义；与状态/复核维度重叠时一并重置避免矛盾空结果
                                    patchUrl(
                                        {
                                            view: "receivable",
                                            due: "overdue",
                                            status: null,
                                            reviewStatus: null,
                                            page: null,
                                        },
                                        { replace: true },
                                    )
                                }}
                            />
                            <MetricFilterItem
                                label="待分配回款"
                                value={
                                    <MoneyValue
                                        value={metrics.unallocatedReceiptTotal}
                                    />
                                }
                                detail="已到账"
                                active={view === "unallocated"}
                                onClick={() => {
                                    patchUrl(
                                        {
                                            view: "unallocated",
                                            due: null,
                                            status: null,
                                            reviewStatus: null,
                                            page: null,
                                        },
                                        { replace: true },
                                    )
                                }}
                            />
                            <MetricFilterItem
                                label="待分配销项发票"
                                value={
                                    <MoneyValue
                                        value={metrics.unallocatedInvoiceTotal}
                                    />
                                }
                                detail={
                                    metrics.cardPendingReviewCount > 0
                                        ? `卡券待复核 ${metrics.cardPendingReviewCount}`
                                        : "独立轨道"
                                }
                                active={view === "sales_invoice"}
                                onClick={() => {
                                    patchUrl(
                                        {
                                            view: "sales_invoice",
                                            due: null,
                                            status: null,
                                            reviewStatus: null,
                                            page: null,
                                        },
                                        { replace: true },
                                    )
                                }}
                            />
                        </MetricStrip>
                    ) : (
                        <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                            {Array.from({ length: 4 }).map((_, i) => (
                                <div
                                    key={i}
                                    className="h-20 animate-pulse rounded-lg bg-muted"
                                />
                            ))}
                        </div>
                    )}

                    <Tabs
                        value={view}
                        onValueChange={(v) => {
                            // 非 receivable 视图隐藏 due/status/reviewStatus，切视图时清除残留
                            const patch: Record<
                                string,
                                string | null | undefined
                            > = {
                                view: v,
                                page: null,
                            }
                            if (v !== "receivable") {
                                patch.due = null
                                patch.status = null
                                patch.reviewStatus = null
                            }
                            patchUrl(patch, { replace: true })
                        }}
                    >
                        <TabsList>
                            {(
                                [
                                    "receivable",
                                    "receipt",
                                    "sales_invoice",
                                    "unallocated",
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
                                {data?.filterSummary ?? "加载中…"}
                                {data ? (
                                    <span className="text-muted-foreground">
                                        {" "}
                                        · 提交方式：{data.submitPolicy.label}
                                    </span>
                                ) : null}
                            </span>
                        }
                        toolbar={
                            <ListToolbar
                                search={
                                    <InputGroup className="max-w-sm">
                                        <InputGroupAddon>
                                            <SearchIcon aria-hidden="true" />
                                        </InputGroupAddon>
                                        <InputGroupInput
                                            ref={searchInputRef}
                                            placeholder="往来主体、销售单、回款单、发票号"
                                            value={searchInput}
                                            onChange={(e) =>
                                                setSearchInput(e.target.value)
                                            }
                                            onKeyDown={(e) => {
                                                if (e.key === "Enter") {
                                                    patchUrl(
                                                        {
                                                            q:
                                                                searchInput.trim() ||
                                                                null,
                                                            page: null,
                                                        },
                                                        { replace: true },
                                                    )
                                                }
                                            }}
                                            aria-label="搜索客户往来"
                                        />
                                    </InputGroup>
                                }
                                filters={
                                    <>
                                        <label className="flex items-center gap-1.5 text-sm">
                                            <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                                往来主体
                                            </span>
                                            <ReceivableCounterpartySearchCombobox
                                                value={
                                                    counterpartyPartyId ||
                                                    undefined
                                                }
                                                onValueChange={(id) => {
                                                    patchUrl(
                                                        {
                                                            counterpartyId:
                                                                id || null,
                                                            page: null,
                                                        },
                                                        { replace: true },
                                                    )
                                                }}
                                                purpose="filter"
                                                className="w-56"
                                                aria-label="筛选往来主体"
                                                placeholder="全部主体"
                                            />
                                        </label>
                                        {view === "receivable" ? (
                                            <>
                                                <label className="flex items-center gap-1.5 text-sm">
                                                    <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                                        到期
                                                    </span>
                                                    <OptionCombobox
                                                        value={due ?? "all"}
                                                        onValueChange={(v) => {
                                                            const next =
                                                                v ?? "all"
                                                            patchUrl(
                                                                {
                                                                    due:
                                                                        next ===
                                                                        "all"
                                                                            ? null
                                                                            : next,
                                                                    page: null,
                                                                },
                                                                {
                                                                    replace: true,
                                                                },
                                                            )
                                                        }}
                                                        options={(
                                                            Object.keys(
                                                                DUE_LABEL,
                                                            ) as DueFilter[]
                                                        ).map((k) => ({
                                                            value: k,
                                                            label: DUE_LABEL[k],
                                                        }))}
                                                        className="w-32"
                                                        size="sm"
                                                        allowClear={false}
                                                        aria-label="筛选到期"
                                                        placeholder="到期"
                                                    />
                                                </label>
                                                <label className="flex items-center gap-1.5 text-sm">
                                                    <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                                        状态
                                                    </span>
                                                    <OptionCombobox
                                                        value={status ?? ""}
                                                        onValueChange={(v) => {
                                                            patchUrl(
                                                                {
                                                                    status:
                                                                        v ||
                                                                        null,
                                                                    page: null,
                                                                },
                                                                {
                                                                    replace: true,
                                                                },
                                                            )
                                                        }}
                                                        options={[
                                                            {
                                                                value: "",
                                                                label: "全部状态",
                                                            },
                                                            {
                                                                value: "open",
                                                                label: "未结",
                                                            },
                                                            {
                                                                value: "partial",
                                                                label: "部分结清",
                                                            },
                                                            {
                                                                value: "settled",
                                                                label: "已结清",
                                                            },
                                                        ]}
                                                        className="w-32"
                                                        size="sm"
                                                        allowClear={false}
                                                        aria-label="筛选状态"
                                                        placeholder="状态"
                                                    />
                                                </label>
                                            </>
                                        ) : null}
                                    </>
                                }
                                secondary={
                                    customerId || view === "receivable" ? (
                                        <>
                                            {customerId ? (
                                                <FilterChip
                                                    label={
                                                        lockedCustomerName
                                                            ? `经营客户 ${lockedCustomerName}`
                                                            : "经营客户锁定"
                                                    }
                                                    onClear={() =>
                                                        patchUrl(
                                                            {
                                                                customerId:
                                                                    null,
                                                            },
                                                            { replace: true },
                                                        )
                                                    }
                                                    clearLabel="清除客户筛选"
                                                />
                                            ) : null}
                                            {view === "receivable" ? (
                                                <label className="flex items-center gap-1.5 text-sm">
                                                    <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                                        复核状态
                                                    </span>
                                                    <OptionCombobox
                                                        value={
                                                            reviewStatus ?? ""
                                                        }
                                                        onValueChange={(v) => {
                                                            patchUrl(
                                                                {
                                                                    reviewStatus:
                                                                        v ||
                                                                        null,
                                                                    page: null,
                                                                },
                                                                {
                                                                    replace: true,
                                                                },
                                                            )
                                                        }}
                                                        options={[
                                                            {
                                                                value: "",
                                                                label: "全部复核状态",
                                                            },
                                                            {
                                                                value: "pending_opening",
                                                                label: "期初待复核",
                                                            },
                                                            {
                                                                value: "reviewed",
                                                                label: "已复核",
                                                            },
                                                            {
                                                                value: "pending_sync_diff",
                                                                label: "同步差额待复核",
                                                            },
                                                        ]}
                                                        className="w-40"
                                                        size="sm"
                                                        allowClear={false}
                                                        aria-label="筛选复核状态"
                                                        placeholder="复核状态"
                                                    />
                                                </label>
                                            ) : null}
                                        </>
                                    ) : undefined
                                }
                                actions={
                                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                        <span aria-live="polite">
                                            共{" "}
                                            {(data?.total ?? 0).toLocaleString(
                                                "zh-CN",
                                            )}{" "}
                                            条
                                        </span>
                                        {hasActiveFilters ? (
                                            <Button
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                onClick={clearFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        ) : null}
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className="text-muted-foreground hover:text-foreground"
                                            onClick={() =>
                                                void listQuery.refetch()
                                            }
                                        >
                                            <RefreshCwIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            刷新
                                        </Button>
                                    </div>
                                }
                            />
                        }
                        table={
                            listQuery.isPending && !data ? (
                                <div className="h-64 animate-pulse rounded-xl bg-muted" />
                            ) : view === "unallocated" && data ? (
                                <div className="space-y-6 p-1">
                                    <Alert variant="info">
                                        <AlertTitle>待核销分区</AlertTitle>
                                        <AlertDescription>
                                            {data.unallocated.note}
                                        </AlertDescription>
                                    </Alert>
                                    <section className="space-y-2">
                                        <h3 className="text-sm font-semibold">
                                            待分配回款
                                            <span className="ml-2 text-xs font-normal text-muted-foreground">
                                                未分配{" "}
                                                <MoneyValue
                                                    value={
                                                        metrics?.unallocatedReceiptTotal ??
                                                        "0"
                                                    }
                                                    className="inline"
                                                />
                                            </span>
                                        </h3>
                                        {data.unallocated.receipts.length ===
                                        0 ? (
                                            <BusinessEmptyState
                                                kind="no-data"
                                                title="无待分配回款"
                                                description="已确认且仍有未分配余额的回款将出现在此。"
                                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                            />
                                        ) : (
                                            <DataTable
                                                data={[
                                                    ...data.unallocated
                                                        .receipts,
                                                ]}
                                                columns={receiptColumns}
                                                getRowId={(r) => r.receiptId}
                                                rowCount={
                                                    data.unallocated.receipts
                                                        .length
                                                }
                                                layout="flush"
                                                density="compact"
                                                defaultColumnPinning={{
                                                    left: ["doc"],
                                                    right: ["actions"],
                                                }}
                                            />
                                        )}
                                    </section>
                                    <Separator />
                                    <section className="space-y-2">
                                        <h3 className="text-sm font-semibold">
                                            待分配销项发票
                                            <span className="ml-2 text-xs font-normal text-muted-foreground">
                                                未分配{" "}
                                                <MoneyValue
                                                    value={
                                                        metrics?.unallocatedInvoiceTotal ??
                                                        "0"
                                                    }
                                                    className="inline"
                                                />
                                                （独立统计）
                                            </span>
                                        </h3>
                                        {data.unallocated.invoices.length ===
                                        0 ? (
                                            <BusinessEmptyState
                                                kind="no-data"
                                                title="无待分配销项发票"
                                                description="已登记蓝票且仍有未分配余额的发票将出现在此。"
                                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                            />
                                        ) : (
                                            <DataTable
                                                data={[
                                                    ...data.unallocated
                                                        .invoices,
                                                ]}
                                                columns={invoiceColumns}
                                                getRowId={(r) => r.invoiceId}
                                                rowCount={
                                                    data.unallocated.invoices
                                                        .length
                                                }
                                                layout="flush"
                                                density="compact"
                                                defaultColumnPinning={{
                                                    left: ["doc"],
                                                    right: ["actions"],
                                                }}
                                            />
                                        )}
                                    </section>
                                </div>
                            ) : data?.total === 0 ? (
                                data.emptyReason === "FILTER_NO_RESULT" ? (
                                    <BusinessEmptyState
                                        kind="filter"
                                        title="无匹配往来记录"
                                        description="无匹配记录，可清除筛选后重试。"
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                        action={
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                onClick={clearFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        }
                                    />
                                ) : (
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="当前范围尚无客户往来记录"
                                        description="可从销售单进入登记；登记后刷新查看。"
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    />
                                )
                            ) : view === "receivable" && data ? (
                                <DataTable
                                    data={[...data.receivables]}
                                    columns={receivableColumns}
                                    getRowId={(r) => r.accountId}
                                    rowCount={data.total}
                                    pagination={pagination}
                                    onPaginationChange={handlePaginationChange}
                                    layout="flush"
                                    density="compact"
                                    defaultColumnPinning={{
                                        left: ["party"],
                                        right: ["actions"],
                                    }}
                                />
                            ) : view === "receipt" && data ? (
                                <DataTable
                                    data={[...data.receipts]}
                                    columns={receiptColumns}
                                    getRowId={(r) => r.receiptId}
                                    rowCount={data.total}
                                    pagination={pagination}
                                    onPaginationChange={handlePaginationChange}
                                    layout="flush"
                                    density="compact"
                                    defaultColumnPinning={{
                                        left: ["doc"],
                                        right: ["actions"],
                                    }}
                                />
                            ) : view === "sales_invoice" && data ? (
                                <DataTable
                                    data={[...data.invoices]}
                                    columns={invoiceColumns}
                                    getRowId={(r) => r.invoiceId}
                                    rowCount={data.total}
                                    pagination={pagination}
                                    onPaginationChange={handlePaginationChange}
                                    layout="flush"
                                    density="compact"
                                    defaultColumnPinning={{
                                        left: ["doc"],
                                        right: ["actions"],
                                    }}
                                />
                            ) : (
                                <div className="h-40 animate-pulse rounded-xl bg-muted" />
                            )
                        }
                    />
                </>
            )}

            <CustomerAccountDetailPreview
                open={preview != null}
                data={detailQuery.data}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                onClose={closePreview}
                onStartSession={startSession}
                onRequestReverse={(request: ReverseRequest) => {
                    if (request.kind === "red_invoice") {
                        setReverseAmount(request.amount ?? "")
                    }
                    setReverseConfirm(request)
                }}
            />

            <ReceivableActionDialogs
                partyPickerOpen={partyPickerOpen}
                partyPickerMode={partyPickerMode}
                selectedPartyId={selectedPartyId}
                createPending={createSession.isPending}
                onPartyPickerOpenChange={setPartyPickerOpen}
                onSelectedPartyIdChange={setSelectedPartyId}
                onStartSession={(mode, partyId) =>
                    void startSession(mode, partyId)
                }
                reverseRequest={reverseConfirm}
                reverseReason={reverseReason}
                reverseAmount={reverseAmount}
                reversePending={reverseMutation.isPending}
                onReverseOpenChange={(open) => {
                    if (!open) {
                        setReverseConfirm(null)
                        setReverseReason("")
                    }
                }}
                onReverseReasonChange={setReverseReason}
                onReverseAmountChange={setReverseAmount}
                onCancelReverse={() => {
                    setReverseConfirm(null)
                    setReverseReason("")
                    setReverseAmount("")
                }}
                onConfirmReverse={() => void confirmReverse()}
            />
        </PageScaffold>
    )
}
