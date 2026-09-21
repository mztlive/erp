"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api/errors"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"

import { downloadQualityCsv } from "../api/dual-caliber"
import type {
    CurrentQualityQuery,
    CurrentQualityRow,
    CurrentQualityView,
    HistoryQualityQuery,
    HistoryQualityRow,
    HistoryQualityView,
    QualityCaliber,
    QualityFilterOption,
} from "../dual-types"
import { toDualEmptyReason } from "../dual-types"
import {
    useCurrentQualityQuery,
    useDualQualityExportMutation,
    useHistoryQualityQuery,
} from "../hooks/dual-queries"
import {
    parseCaliber,
    parseCsvIds,
    parseCurrentDimension,
    parseCurrentSort,
    parseDualBoolean,
    parseDualPage,
    parseDualPageSize,
    parseHistoryDimension,
    parseHistorySort,
    serializeCsvIds,
} from "../lib/dual-url-state"
import { DualCaliberSwitch } from "./dual-caliber-switch"

const CURRENT_SORT_OPTIONS = [
    { value: "orderCount:desc", label: "订单数降序" },
    { value: "orderCount:asc", label: "订单数升序" },
    { value: "grossTotal:desc", label: "含税总额降序" },
    { value: "grossTotal:asc", label: "含税总额升序" },
    { value: "customerNo:asc", label: "客户编号升序" },
    { value: "label:asc", label: "分组名称升序" },
    { value: "customerCount:desc", label: "客户数降序" },
]

const HISTORY_SORT_OPTIONS = [
    { value: "orderCount:desc", label: "订单数降序" },
    { value: "orderCount:asc", label: "订单数升序" },
    { value: "grossTotal:desc", label: "含税总额降序" },
    { value: "grossTotal:asc", label: "含税总额升序" },
    { value: "label:asc", label: "分组名称升序" },
]

function isScopeChanged(error: unknown): boolean {
    return isDataScopeChanged(error)
}

function toggleId(ids: readonly string[], id: string): string[] {
    return ids.includes(id) ? ids.filter((v) => v !== id) : [...ids, id]
}

/**
 * S3-05 M10 双口径独立查询区：当前负责与历史贡献各走本口径端点，
 * 人员／组织／业务条件全部进入 URL 与 Query key；候选各自区分；
 * 无范围／筛选为空／请求失败三分呈现；390px 下无页面横溢。
 */
export function DualQualitySection({
    from,
    to,
}: {
    from?: string
    to?: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const caliber: QualityCaliber = parseCaliber(searchParams.get("caliber"))

    function patchDual(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean; scroll?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams },
            { ...patch, page: null },
            { replace: true, scroll: false, ...options },
        )
    }

    function handleCaliberChange(next: QualityCaliber) {
        // 切换口径即切换查询与缓存；对方口径的版本与分页不带入新口径。
        patchDual({
            caliber: next === "current" ? null : "history",
            scopeVersion: null,
            dualPage: null,
            ownerGroup: null,
            attributionGroup: null,
        })
    }

    if (!from || !to) {
        return (
            <section
                aria-label="客户经营质量双口径"
                className="flex min-w-0 flex-col gap-3"
            >
                <DualCaliberSwitch
                    caliber={caliber}
                    onChange={handleCaliberChange}
                />
                <BusinessEmptyState
                    kind="no-data"
                    title="请先确定统计期间"
                    description="选择起止日期后，方可按口径查询当前负责与历史贡献。"
                />
            </section>
        )
    }

    return (
        <section
            aria-label="客户经营质量双口径"
            className="flex min-w-0 flex-col gap-3"
        >
            <DualCaliberSwitch
                caliber={caliber}
                onChange={handleCaliberChange}
            />
            {caliber === "current" ? (
                <CurrentCaliberPanel
                    from={from}
                    to={to}
                    patchDual={patchDual}
                />
            ) : (
                <HistoryCaliberPanel
                    from={from}
                    to={to}
                    patchDual={patchDual}
                />
            )}
        </section>
    )
}

type PatchDual = (patch: Record<string, string | null | undefined>) => void

function useScopeVersionWriteBack(
    scopeVersion: string | undefined,
    page: number,
    patchDual: PatchDual,
    searchParams: URLSearchParams,
) {
    const writtenRef = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (!scopeVersion || page !== 1) return
        if (searchParams.get("scopeVersion") === scopeVersion) return
        if (writtenRef.current === scopeVersion) return
        writtenRef.current = scopeVersion
        patchDual({ scopeVersion })
    }, [scopeVersion, page, patchDual, searchParams])
}

function CandidatePicker({
    idPrefix,
    title,
    options,
    selected,
    emptyLabel,
    onToggle,
}: {
    idPrefix: string
    title: string
    options: readonly QualityFilterOption[]
    selected: readonly string[]
    emptyLabel: string
    onToggle: (id: string) => void
}) {
    return (
        <fieldset className="min-w-0 rounded-xl border border-border p-3">
            <legend className="px-1 text-xs font-medium text-muted-foreground">
                {title}（{options.length}）
            </legend>
            {options.length === 0 ? (
                <p className="text-xs text-muted-foreground">{emptyLabel}</p>
            ) : (
                <ul className="flex max-h-40 min-w-0 flex-col gap-1 overflow-y-auto">
                    {options.map((option) => {
                        const checked = selected.includes(option.value)
                        const inputId = `${idPrefix}-option-${toAutomationIdSegment(option.value)}`
                        return (
                            <li key={option.value} className="min-w-0">
                                <label
                                    htmlFor={inputId}
                                    className="flex min-w-0 cursor-pointer items-center gap-2 text-[13px]"
                                >
                                    <input
                                        id={inputId}
                                        type="checkbox"
                                        checked={checked}
                                        onChange={() => onToggle(option.value)}
                                        className="size-4 shrink-0"
                                    />
                                    <span className="min-w-0 truncate">
                                        {option.label}
                                    </span>
                                </label>
                            </li>
                        )
                    })}
                </ul>
            )}
        </fieldset>
    )
}

function SummaryStrip({
    scopeSummary,
    ownershipBasis,
    asOf,
    filterSummary,
    objectCount,
    orderCount,
    grossTotal,
    unpricedCount,
    policyVersion,
    organizationVersion,
    scopeVersion,
}: {
    scopeSummary: string
    ownershipBasis: string
    asOf: string
    filterSummary: string
    objectCount: number
    orderCount: number
    grossTotal: string
    unpricedCount: number
    policyVersion: number
    organizationVersion: number
    scopeVersion: string
}) {
    return (
        <dl className="grid min-w-0 grid-cols-2 gap-2 rounded-xl border border-border p-3 text-[13px] sm:grid-cols-4">
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">范围</dt>
                <dd className="truncate font-medium">{scopeSummary}</dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">归属口径</dt>
                <dd className="truncate font-medium">{ownershipBasis}</dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    对象数 / 订单数
                </dt>
                <dd className="num font-medium">
                    {objectCount} / {orderCount}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    含税总额 / 缺版本
                </dt>
                <dd className="num font-medium">
                    <MoneyValue value={grossTotal} taxBasis="gross" /> /{" "}
                    {unpricedCount}
                </dd>
            </div>
            <div className="col-span-2 min-w-0 sm:col-span-4">
                <dt className="text-xs text-muted-foreground">筛选</dt>
                <dd className="break-all">{filterSummary}</dd>
            </div>
            <div className="col-span-2 min-w-0 text-xs text-muted-foreground sm:col-span-4">
                授权时点 {asOf || "—"} · 权限版本 {policyVersion} · 组织版本{" "}
                {organizationVersion} · 范围版本{" "}
                <span className="num break-all">{scopeVersion || "—"}</span>
            </div>
        </dl>
    )
}

function Pager({
    idPrefix,
    page,
    pageSize,
    total,
    scopeVersion,
    patchDual,
}: {
    idPrefix: string
    page: number
    pageSize: number
    total: number
    scopeVersion?: string
    patchDual: PatchDual
}) {
    const pageCount = Math.max(1, Math.ceil(total / pageSize))
    const safePage = Math.min(page, pageCount)
    return (
        <div className="flex min-w-0 flex-wrap items-center gap-2 text-[13px]">
            <Button
                id={`${idPrefix}-prev-page`}
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage <= 1}
                onClick={() =>
                    patchDual({
                        dualPage:
                            safePage - 1 <= 1 ? null : String(safePage - 1),
                        scopeVersion: scopeVersion ?? null,
                    })
                }
            >
                上一页
            </Button>
            <span className="num text-muted-foreground">
                第 {safePage} / {pageCount} 页 · 共 {total} 行
            </span>
            <Button
                id={`${idPrefix}-next-page`}
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage >= pageCount}
                onClick={() =>
                    patchDual({
                        dualPage: String(safePage + 1),
                        scopeVersion: scopeVersion ?? null,
                    })
                }
            >
                下一页
            </Button>
        </div>
    )
}

function ScopeChangedPanel({
    idPrefix,
    error,
    onRefreshFromFirst,
    onRetry,
}: {
    idPrefix: string
    error: unknown
    onRefreshFromFirst: () => void
    onRetry: () => void
}) {
    return (
        <BusinessFailureState
            kind="conflict"
            title="数据范围已变化"
            error={error}
            action={
                <div className="flex flex-wrap gap-2">
                    <Button
                        id={`${idPrefix}-scope-changed-refresh`}
                        type="button"
                        size="sm"
                        onClick={onRefreshFromFirst}
                    >
                        从第一页刷新
                    </Button>
                    <Button
                        id={`${idPrefix}-scope-changed-retry`}
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                </div>
            }
        />
    )
}

function CurrentCaliberPanel({
    from,
    to,
    patchDual,
}: {
    from: string
    to: string
    patchDual: PatchDual
}) {
    const searchParams = useSearchParams()
    const ownerIds = parseCsvIds(searchParams.get("ownerUserIds"))
    const orgIds = parseCsvIds(searchParams.get("orgUnitIds"))
    const includeDescendants = parseDualBoolean(
        searchParams.get("includeDescendants"),
    )
    const ownerGroup = searchParams.get("ownerGroup") ?? undefined
    const customerId = searchParams.get("dualCustomerId") ?? undefined
    const qParam = searchParams.get("dualQ") ?? ""
    const dimension = parseCurrentDimension(searchParams.get("dualDimension"))
    const sort = parseCurrentSort(searchParams.get("dualSort"))
    const page = parseDualPage(searchParams.get("dualPage"))
    const pageSize = parseDualPageSize(searchParams.get("dualPageSize"))
    const scopeVersion = searchParams.get("scopeVersion") ?? undefined

    const [qDraft, setQDraft] = React.useState(qParam)
    const [idDraft, setIdDraft] = React.useState("")
    React.useEffect(() => {
        if (document.activeElement?.id !== "customers-quality-dual-search") {
            setQDraft(qParam)
        }
    }, [qParam])

    const query: CurrentQualityQuery = React.useMemo(
        () => ({
            from,
            to,
            ownerUserIds: ownerIds.length ? ownerIds : undefined,
            orgUnitIds: orgIds.length ? orgIds : undefined,
            includeDescendants,
            customerId,
            ownerGroup,
            q: qParam || undefined,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        }),
        [
            from,
            to,
            ownerIds,
            orgIds,
            includeDescendants,
            customerId,
            ownerGroup,
            qParam,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        ],
    )
    const viewQuery = useCurrentQualityQuery(query)
    useScopeVersionWriteBack(
        viewQuery.data?.scopeVersion,
        page,
        patchDual,
        searchParams,
    )
    const exportMutation = useDualQualityExportMutation("current")
    const [exportError, setExportError] = React.useState<string | null>(null)
    const [exportDone, setExportDone] = React.useState<string | null>(null)

    const data: CurrentQualityView | undefined = viewQuery.data
    const emptyReason = toDualEmptyReason(data?.emptyReason)
    const hasFilters =
        ownerIds.length > 0 ||
        orgIds.length > 0 ||
        qParam !== "" ||
        ownerGroup != null ||
        customerId != null

    function applySearch() {
        patchDual({
            dualQ: qDraft.trim() || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    function applyIdDraft() {
        const ids = parseCsvIds(idDraft)
        if (ids.length === 0) return
        const merged = serializeCsvIds([...ownerIds, ...ids])
        setIdDraft("")
        patchDual({
            ownerUserIds: merged || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    async function handleExport() {
        if (!data) return
        setExportError(null)
        setExportDone(null)
        try {
            const file = await exportMutation.mutateAsync({ current: query })
            downloadQualityCsv(file.csvContent, file.fileName)
            setExportDone(
                `已导出 ${file.rowCount} 行（${file.fileName}，${file.generatedAt} 生成，版本已绑定）。`,
            )
        } catch (error) {
            setExportError(getErrorMessage(error, "导出失败，请重试。"))
        }
    }

    return (
        <div className="flex min-w-0 flex-col gap-3">
            <p className="text-xs text-muted-foreground">
                当前负责口径：按客户现任主责与主责所属组织分组汇总；只看现任归属，不读取历史冻结快照。
            </p>

            <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-search"
                        className="sr-only"
                    >
                        搜索客户或单号
                    </label>
                    <input
                        id="customers-quality-dual-search"
                        type="search"
                        value={qDraft}
                        onChange={(e) => setQDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applySearch()
                            }
                        }}
                        placeholder="客户编号 / 名称 / 单号"
                        className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 text-sm"
                    />
                    <Button
                        id="customers-quality-dual-apply"
                        type="button"
                        size="sm"
                        onClick={applySearch}
                    >
                        查询
                    </Button>
                </div>
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-owner-id"
                        className="sr-only"
                    >
                        现任负责人 ID
                    </label>
                    <input
                        id="customers-quality-dual-owner-id"
                        value={idDraft}
                        onChange={(e) => setIdDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applyIdDraft()
                            }
                        }}
                        placeholder="现任负责人 ID（逗号分隔）"
                        className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 font-mono text-sm"
                    />
                    <Button
                        id="customers-quality-dual-owner-add"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={applyIdDraft}
                    >
                        添加
                    </Button>
                </div>
            </div>

            <div className="flex min-w-0 flex-wrap items-center gap-2 text-[13px]">
                <label
                    htmlFor="customers-quality-dual-dimension"
                    className="text-muted-foreground"
                >
                    分组
                </label>
                <select
                    id="customers-quality-dual-dimension"
                    value={dimension}
                    onChange={(e) =>
                        patchDual({
                            dualDimension:
                                e.target.value === "customer"
                                    ? null
                                    : e.target.value,
                            ownerGroup: null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }
                    className="h-9 min-w-0 rounded-lg border border-border bg-background px-2"
                >
                    <option value="customer">按客户</option>
                    <option value="owner_user">按现任负责人</option>
                    <option value="owner_org">按现任组织</option>
                </select>
                <label
                    htmlFor="customers-quality-dual-sort"
                    className="text-muted-foreground"
                >
                    排序
                </label>
                <select
                    id="customers-quality-dual-sort"
                    value={sort}
                    onChange={(e) =>
                        patchDual({
                            dualSort:
                                e.target.value === "orderCount:desc"
                                    ? null
                                    : e.target.value,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }
                    className="h-9 min-w-0 rounded-lg border border-border bg-background px-2"
                >
                    {CURRENT_SORT_OPTIONS.map((o) => (
                        <option key={o.value} value={o.value}>
                            {o.label}
                        </option>
                    ))}
                </select>
                <label className="flex cursor-pointer items-center gap-1.5">
                    <input
                        id="customers-quality-dual-descendants"
                        type="checkbox"
                        checked={includeDescendants === true}
                        onChange={(e) =>
                            patchDual({
                                includeDescendants: e.target.checked
                                    ? "true"
                                    : null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                        className="size-4"
                    />
                    组织含下级
                </label>
                {hasFilters ? (
                    <Button
                        id="customers-quality-dual-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                ownerUserIds: null,
                                orgUnitIds: null,
                                includeDescendants: null,
                                ownerGroup: null,
                                dualCustomerId: null,
                                dualQ: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除筛选
                    </Button>
                ) : null}
            </div>

            {ownerGroup ? (
                <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border p-2 text-[13px]">
                    <span className="min-w-0 truncate">
                        现任分组下钻：{ownerGroup}
                    </span>
                    <Button
                        id="customers-quality-dual-drill-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                ownerGroup: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除下钻
                    </Button>
                </div>
            ) : null}

            {viewQuery.isError ? (
                isScopeChanged(viewQuery.error) ? (
                    <ScopeChangedPanel
                        idPrefix="customers-quality-dual-current"
                        error={viewQuery.error}
                        onRefreshFromFirst={() =>
                            patchDual({
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                        onRetry={() => void viewQuery.refetch()}
                    />
                ) : (
                    <BusinessFailureState
                        title="当前负责口径加载失败"
                        error={viewQuery.error}
                        onRetry={() => void viewQuery.refetch()}
                    />
                )
            ) : viewQuery.isPending || !data ? (
                <p className="text-sm text-muted-foreground">
                    正在加载当前负责口径…
                </p>
            ) : emptyReason === "no-scope" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无客户数据范围"
                    description="当前角色无客户数据范围，请申请权限。历史贡献口径不受此影响，可切换查看。"
                />
            ) : (
                <>
                    <SummaryStrip
                        scopeSummary={data.scopeSummary}
                        ownershipBasis={data.ownershipBasis}
                        asOf={data.asOf}
                        filterSummary={data.filterSummary}
                        objectCount={data.totals.objectCount}
                        orderCount={data.totals.orderCount}
                        grossTotal={data.totals.grossTotal}
                        unpricedCount={data.totals.unpricedCount}
                        policyVersion={data.policyVersion}
                        organizationVersion={data.organizationVersion}
                        scopeVersion={data.scopeVersion}
                    />
                    <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                        <CandidatePicker
                            idPrefix="customers-quality-dual-owner"
                            title="现任负责人候选"
                            options={data.ownerOptions}
                            selected={ownerIds}
                            emptyLabel="当前结果无负责人候选。"
                            onToggle={(id) => {
                                const merged = serializeCsvIds(
                                    toggleId(ownerIds, id),
                                )
                                patchDual({
                                    ownerUserIds: merged || null,
                                    scopeVersion: null,
                                    dualPage: null,
                                })
                            }}
                        />
                        <CandidatePicker
                            idPrefix="customers-quality-dual-org"
                            title="现任组织候选"
                            options={data.orgOptions}
                            selected={orgIds}
                            emptyLabel="当前结果无组织候选。"
                            onToggle={(id) => {
                                const merged = serializeCsvIds(
                                    toggleId(orgIds, id),
                                )
                                patchDual({
                                    orgUnitIds: merged || null,
                                    scopeVersion: null,
                                    dualPage: null,
                                })
                            }}
                        />
                    </div>
                    {emptyReason === "filtered-empty" ||
                    data.rows.total === 0 ? (
                        <BusinessEmptyState
                            kind="filter"
                            title="当前筛选无客户结果"
                            description={`筛选：${data.filterSummary}`}
                            action={
                                <Button
                                    id="customers-quality-dual-empty-clear"
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() =>
                                        patchDual({
                                            ownerUserIds: null,
                                            orgUnitIds: null,
                                            includeDescendants: null,
                                            ownerGroup: null,
                                            dualCustomerId: null,
                                            dualQ: null,
                                            scopeVersion: null,
                                            dualPage: null,
                                        })
                                    }
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : emptyReason === "no-data" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="期间内无授权经营记录"
                            description="可调整统计期间或数据范围后重查。"
                        />
                    ) : (
                        <CurrentRowsTable
                            items={data.rows.items}
                            dimension={dimension}
                            patchDual={patchDual}
                        />
                    )}
                    <Pager
                        idPrefix="customers-quality-dual-current"
                        page={page}
                        pageSize={pageSize}
                        total={data.rows.total}
                        scopeVersion={data.scopeVersion}
                        patchDual={patchDual}
                    />
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                        <Button
                            id="customers-quality-dual-current-export"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={
                                !data.canExport ||
                                data.rows.total === 0 ||
                                exportMutation.isPending
                            }
                            onClick={() => void handleExport()}
                        >
                            {exportMutation.isPending
                                ? "导出中…"
                                : "导出当前口径 CSV"}
                        </Button>
                        {exportError ? (
                            <span className="min-w-0 break-all text-[13px] text-destructive">
                                {exportError}
                            </span>
                        ) : null}
                        {exportDone ? (
                            <span className="min-w-0 break-all text-[13px] text-muted-foreground">
                                {exportDone}
                            </span>
                        ) : null}
                    </div>
                </>
            )}
        </div>
    )
}

function CurrentRowsTable({
    items,
    dimension,
    patchDual,
}: {
    items: readonly CurrentQualityRow[]
    dimension: string
    patchDual: PatchDual
}) {
    const grouped = dimension !== "customer"
    return (
        <div className="min-w-0 overflow-x-auto rounded-xl border border-border">
            <table className="w-full min-w-[560px] border-collapse text-sm">
                <thead>
                    <tr className="border-b border-border text-left text-xs text-muted-foreground">
                        <th className="px-3 py-2 font-medium">
                            {grouped ? "分组" : "客户"}
                        </th>
                        <th className="px-3 py-2 font-medium">现任负责人</th>
                        <th className="px-3 py-2 font-medium">现任组织</th>
                        {grouped ? (
                            <th className="px-3 py-2 text-right font-medium">
                                客户数
                            </th>
                        ) : null}
                        <th className="px-3 py-2 text-right font-medium">
                            订单数
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            含税总额
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            缺版本
                        </th>
                        {grouped ? (
                            <th className="px-3 py-2 text-right font-medium">
                                下钻
                            </th>
                        ) : null}
                    </tr>
                </thead>
                <tbody>
                    {items.map((row) => {
                        const drill =
                            dimension === "owner_user" && row.groupId != null
                                ? `user:${row.groupId}`
                                : dimension === "owner_org" &&
                                    row.groupId != null
                                  ? `org:${row.groupId}`
                                  : null
                        return (
                            <tr
                                key={row.rowId}
                                className="border-b border-border last:border-0"
                            >
                                <td className="max-w-48 px-3 py-2">
                                    <div className="truncate font-medium">
                                        {grouped
                                            ? (row.label ?? row.rowId)
                                            : (row.customerName ?? row.rowId)}
                                    </div>
                                    {!grouped && row.customerNo ? (
                                        <div className="num truncate text-xs text-muted-foreground">
                                            {row.customerNo}
                                        </div>
                                    ) : null}
                                </td>
                                <td className="max-w-40 truncate px-3 py-2 text-[13px]">
                                    {row.ownerUserName ??
                                        row.ownerUserId ??
                                        "—"}
                                </td>
                                <td className="max-w-40 truncate px-3 py-2 text-[13px]">
                                    {row.ownerOrgUnitName ??
                                        row.ownerOrgUnitId ??
                                        "—"}
                                </td>
                                {grouped ? (
                                    <td className="num px-3 py-2 text-right">
                                        {row.customerCount ?? "—"}
                                    </td>
                                ) : null}
                                <td className="num px-3 py-2 text-right">
                                    {row.orderCount}
                                </td>
                                <td className="px-3 py-2 text-right">
                                    <MoneyValue
                                        value={row.grossTotal}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="num px-3 py-2 text-right">
                                    {row.unpricedCount}
                                </td>
                                {grouped ? (
                                    <td className="px-3 py-2 text-right">
                                        {drill ? (
                                            <Button
                                                id={`customers-quality-dual-drill-${toAutomationIdSegment(row.rowId)}`}
                                                type="button"
                                                variant="link"
                                                size="xs"
                                                onClick={() =>
                                                    patchDual({
                                                        ownerGroup: drill,
                                                        scopeVersion: null,
                                                        dualPage: null,
                                                    })
                                                }
                                            >
                                                下钻
                                            </Button>
                                        ) : (
                                            "—"
                                        )}
                                    </td>
                                ) : null}
                            </tr>
                        )
                    })}
                </tbody>
            </table>
        </div>
    )
}

function HistoryCaliberPanel({
    from,
    to,
    patchDual,
}: {
    from: string
    to: string
    patchDual: PatchDual
}) {
    const searchParams = useSearchParams()
    const userIds = parseCsvIds(searchParams.get("attributionUserIds"))
    const orgIds = parseCsvIds(searchParams.get("attributionOrgUnitIds"))
    const attributionGroup = searchParams.get("attributionGroup") ?? undefined
    const customerId = searchParams.get("dualCustomerId") ?? undefined
    const qParam = searchParams.get("dualQ") ?? ""
    const dimension = parseHistoryDimension(searchParams.get("dualDimension"))
    const sort = parseHistorySort(searchParams.get("dualSort"))
    const page = parseDualPage(searchParams.get("dualPage"))
    const pageSize = parseDualPageSize(searchParams.get("dualPageSize"))
    const scopeVersion = searchParams.get("scopeVersion") ?? undefined

    const [qDraft, setQDraft] = React.useState(qParam)
    const [idDraft, setIdDraft] = React.useState("")
    React.useEffect(() => {
        if (document.activeElement?.id !== "customers-quality-dual-search") {
            setQDraft(qParam)
        }
    }, [qParam])

    const query: HistoryQualityQuery = React.useMemo(
        () => ({
            from,
            to,
            attributionUserIds: userIds.length ? userIds : undefined,
            attributionOrgUnitIds: orgIds.length ? orgIds : undefined,
            attributionGroup,
            customerId,
            q: qParam || undefined,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        }),
        [
            from,
            to,
            userIds,
            orgIds,
            attributionGroup,
            customerId,
            qParam,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        ],
    )
    const viewQuery = useHistoryQualityQuery(query)
    useScopeVersionWriteBack(
        viewQuery.data?.scopeVersion,
        page,
        patchDual,
        searchParams,
    )
    const exportMutation = useDualQualityExportMutation("history")
    const [exportError, setExportError] = React.useState<string | null>(null)
    const [exportDone, setExportDone] = React.useState<string | null>(null)

    const data: HistoryQualityView | undefined = viewQuery.data
    const emptyReason = toDualEmptyReason(data?.emptyReason)
    const hasFilters =
        userIds.length > 0 ||
        orgIds.length > 0 ||
        qParam !== "" ||
        attributionGroup != null ||
        customerId != null

    function applySearch() {
        patchDual({
            dualQ: qDraft.trim() || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    function applyIdDraft() {
        const ids = parseCsvIds(idDraft)
        if (ids.length === 0) return
        const merged = serializeCsvIds([...userIds, ...ids])
        setIdDraft("")
        patchDual({
            attributionUserIds: merged || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    async function handleExport() {
        if (!data) return
        setExportError(null)
        setExportDone(null)
        try {
            const file = await exportMutation.mutateAsync({ history: query })
            downloadQualityCsv(file.csvContent, file.fileName)
            setExportDone(
                `已导出 ${file.rowCount} 行（${file.fileName}，${file.generatedAt} 生成，版本已绑定）。`,
            )
        } catch (error) {
            setExportError(getErrorMessage(error, "导出失败，请重试。"))
        }
    }

    return (
        <div className="flex min-w-0 flex-col gap-3">
            <p className="text-xs text-muted-foreground">
                历史贡献口径：按销售单首次生效时冻结的负责人与组织祖先路径分组汇总；人员调岗、客户换任不改写历史，永不用现任负责人回填。
            </p>

            <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-search"
                        className="sr-only"
                    >
                        搜索客户或单号
                    </label>
                    <input
                        id="customers-quality-dual-search"
                        type="search"
                        value={qDraft}
                        onChange={(e) => setQDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applySearch()
                            }
                        }}
                        placeholder="客户名称 / 单号"
                        className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 text-sm"
                    />
                    <Button
                        id="customers-quality-dual-apply"
                        type="button"
                        size="sm"
                        onClick={applySearch}
                    >
                        查询
                    </Button>
                </div>
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-attribution-id"
                        className="sr-only"
                    >
                        历史归属销售 ID
                    </label>
                    <input
                        id="customers-quality-dual-attribution-id"
                        value={idDraft}
                        onChange={(e) => setIdDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applyIdDraft()
                            }
                        }}
                        placeholder="历史归属销售 ID（逗号分隔）"
                        className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 font-mono text-sm"
                    />
                    <Button
                        id="customers-quality-dual-attribution-add"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={applyIdDraft}
                    >
                        添加
                    </Button>
                </div>
            </div>

            <div className="flex min-w-0 flex-wrap items-center gap-2 text-[13px]">
                <label
                    htmlFor="customers-quality-dual-dimension"
                    className="text-muted-foreground"
                >
                    分组
                </label>
                <select
                    id="customers-quality-dual-dimension"
                    value={dimension}
                    onChange={(e) =>
                        patchDual({
                            dualDimension:
                                e.target.value === "attribution_user"
                                    ? null
                                    : e.target.value,
                            attributionGroup: null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }
                    className="h-9 min-w-0 rounded-lg border border-border bg-background px-2"
                >
                    <option value="attribution_user">按历史归属销售</option>
                    <option value="attribution_org">按历史归属组织</option>
                </select>
                <label
                    htmlFor="customers-quality-dual-sort"
                    className="text-muted-foreground"
                >
                    排序
                </label>
                <select
                    id="customers-quality-dual-sort"
                    value={sort}
                    onChange={(e) =>
                        patchDual({
                            dualSort:
                                e.target.value === "orderCount:desc"
                                    ? null
                                    : e.target.value,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }
                    className="h-9 min-w-0 rounded-lg border border-border bg-background px-2"
                >
                    {HISTORY_SORT_OPTIONS.map((o) => (
                        <option key={o.value} value={o.value}>
                            {o.label}
                        </option>
                    ))}
                </select>
                {hasFilters ? (
                    <Button
                        id="customers-quality-dual-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                attributionUserIds: null,
                                attributionOrgUnitIds: null,
                                attributionGroup: null,
                                dualCustomerId: null,
                                dualQ: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除筛选
                    </Button>
                ) : null}
            </div>

            {attributionGroup ? (
                <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border p-2 text-[13px]">
                    <span className="min-w-0 truncate">
                        历史分组下钻：{attributionGroup}
                    </span>
                    <Button
                        id="customers-quality-dual-drill-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                attributionGroup: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除下钻
                    </Button>
                </div>
            ) : null}

            {viewQuery.isError ? (
                isScopeChanged(viewQuery.error) ? (
                    <ScopeChangedPanel
                        idPrefix="customers-quality-dual-history"
                        error={viewQuery.error}
                        onRefreshFromFirst={() =>
                            patchDual({
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                        onRetry={() => void viewQuery.refetch()}
                    />
                ) : (
                    <BusinessFailureState
                        title="历史贡献口径加载失败"
                        error={viewQuery.error}
                        onRetry={() => void viewQuery.refetch()}
                    />
                )
            ) : viewQuery.isPending || !data ? (
                <p className="text-sm text-muted-foreground">
                    正在加载历史贡献口径…
                </p>
            ) : emptyReason === "no-scope" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无销售单数据范围"
                    description="当前角色无可查看的销售单范围，请申请权限。当前负责口径不受此影响，可切换查看。"
                />
            ) : (
                <>
                    <SummaryStrip
                        scopeSummary={data.scopeSummary}
                        ownershipBasis={data.ownershipBasis}
                        asOf={data.asOf}
                        filterSummary={data.filterSummary}
                        objectCount={data.totals.objectCount}
                        orderCount={data.totals.orderCount}
                        grossTotal={data.totals.grossTotal}
                        unpricedCount={data.totals.unpricedCount}
                        policyVersion={data.policyVersion}
                        organizationVersion={data.organizationVersion}
                        scopeVersion={data.scopeVersion}
                    />
                    <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                        <CandidatePicker
                            idPrefix="customers-quality-dual-attribution-user"
                            title="历史归属销售候选"
                            options={data.attributionUserOptions}
                            selected={userIds}
                            emptyLabel="当前结果无历史销售候选。"
                            onToggle={(id) => {
                                const merged = serializeCsvIds(
                                    toggleId(userIds, id),
                                )
                                patchDual({
                                    attributionUserIds: merged || null,
                                    scopeVersion: null,
                                    dualPage: null,
                                })
                            }}
                        />
                        <CandidatePicker
                            idPrefix="customers-quality-dual-attribution-org"
                            title="历史归属组织候选"
                            options={data.attributionOrgOptions}
                            selected={orgIds}
                            emptyLabel="当前结果无历史组织候选。"
                            onToggle={(id) => {
                                const merged = serializeCsvIds(
                                    toggleId(orgIds, id),
                                )
                                patchDual({
                                    attributionOrgUnitIds: merged || null,
                                    scopeVersion: null,
                                    dualPage: null,
                                })
                            }}
                        />
                    </div>
                    {emptyReason === "filtered-empty" ||
                    data.rows.total === 0 ? (
                        <BusinessEmptyState
                            kind="filter"
                            title="当前筛选无历史贡献结果"
                            description={`筛选：${data.filterSummary}`}
                            action={
                                <Button
                                    id="customers-quality-dual-empty-clear"
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() =>
                                        patchDual({
                                            attributionUserIds: null,
                                            attributionOrgUnitIds: null,
                                            attributionGroup: null,
                                            dualCustomerId: null,
                                            dualQ: null,
                                            scopeVersion: null,
                                            dualPage: null,
                                        })
                                    }
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : emptyReason === "no-data" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="期间内无授权历史订单"
                            description="可调整统计期间或数据范围后重查。"
                        />
                    ) : (
                        <HistoryRowsTable
                            items={data.rows.items}
                            dimension={dimension}
                            patchDual={patchDual}
                        />
                    )}
                    <Pager
                        idPrefix="customers-quality-dual-history"
                        page={page}
                        pageSize={pageSize}
                        total={data.rows.total}
                        scopeVersion={data.scopeVersion}
                        patchDual={patchDual}
                    />
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                        <Button
                            id="customers-quality-dual-history-export"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={
                                !data.canExport ||
                                data.rows.total === 0 ||
                                exportMutation.isPending
                            }
                            onClick={() => void handleExport()}
                        >
                            {exportMutation.isPending
                                ? "导出中…"
                                : "导出历史口径 CSV"}
                        </Button>
                        {exportError ? (
                            <span className="min-w-0 break-all text-[13px] text-destructive">
                                {exportError}
                            </span>
                        ) : null}
                        {exportDone ? (
                            <span className="min-w-0 break-all text-[13px] text-muted-foreground">
                                {exportDone}
                            </span>
                        ) : null}
                    </div>
                </>
            )}
        </div>
    )
}

function HistoryRowsTable({
    items,
    dimension,
    patchDual,
}: {
    items: readonly HistoryQualityRow[]
    dimension: string
    patchDual: PatchDual
}) {
    return (
        <div className="min-w-0 overflow-x-auto rounded-xl border border-border">
            <table className="w-full min-w-[560px] border-collapse text-sm">
                <thead>
                    <tr className="border-b border-border text-left text-xs text-muted-foreground">
                        <th className="px-3 py-2 font-medium">
                            {dimension === "attribution_user"
                                ? "历史归属销售"
                                : "历史归属组织"}
                        </th>
                        <th className="px-3 py-2 font-medium">归属客户</th>
                        <th className="px-3 py-2 text-right font-medium">
                            订单数
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            含税总额
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            缺版本
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            下钻
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {items.map((row) => {
                        const drill =
                            row.groupId != null
                                ? dimension === "attribution_user"
                                    ? `attribution_user:${row.groupId}`
                                    : `attribution_org:${row.groupId}`
                                : null
                        const identity =
                            dimension === "attribution_user"
                                ? (row.attributionUserName ??
                                  row.attributionUserId ??
                                  row.label ??
                                  row.rowId)
                                : (row.attributionOrgUnitName ??
                                  row.attributionOrgUnitId ??
                                  row.label ??
                                  row.rowId)
                        return (
                            <tr
                                key={row.rowId}
                                className="border-b border-border last:border-0"
                            >
                                <td className="max-w-48 px-3 py-2">
                                    <div className="truncate font-medium">
                                        {identity}
                                    </div>
                                    {row.orderNo ? (
                                        <div className="num truncate text-xs text-muted-foreground">
                                            {row.orderNo}
                                        </div>
                                    ) : null}
                                </td>
                                <td className="max-w-40 truncate px-3 py-2 text-[13px]">
                                    {row.customerName ?? row.customerId ?? "—"}
                                </td>
                                <td className="num px-3 py-2 text-right">
                                    {row.orderCount ?? "—"}
                                </td>
                                <td className="px-3 py-2 text-right">
                                    <MoneyValue
                                        value={row.grossTotal}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="num px-3 py-2 text-right">
                                    {row.unpricedCount}
                                </td>
                                <td className="px-3 py-2 text-right">
                                    {drill ? (
                                        <Button
                                            id={`customers-quality-dual-drill-${toAutomationIdSegment(row.rowId)}`}
                                            type="button"
                                            variant="link"
                                            size="xs"
                                            onClick={() =>
                                                patchDual({
                                                    attributionGroup: drill,
                                                    scopeVersion: null,
                                                    dualPage: null,
                                                })
                                            }
                                        >
                                            下钻
                                        </Button>
                                    ) : (
                                        "—"
                                    )}
                                </td>
                            </tr>
                        )
                    })}
                </tbody>
            </table>
        </div>
    )
}
