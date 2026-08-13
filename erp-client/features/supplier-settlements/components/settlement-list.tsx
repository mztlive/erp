"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnPinningState, PaginationState } from "@tanstack/react-table"
import {
    ExternalLinkIcon,
    PlusIcon,
    RefreshCwIcon,
    SearchIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    DocumentTotals,
    FormalActionResult,
    GuardedBusinessAction,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    MoneyValue,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { CreateDraftDialog } from "@/features/supplier-settlements/components/create-draft-dialog"
import { CrossEntryBanner } from "@/features/supplier-settlements/components/cross-entry-banner"
import { useSettlementListQuery } from "@/features/supplier-settlements/hooks/queries"
import { useSettlementListColumns } from "@/features/supplier-settlements/hooks/use-settlement-list-columns"
import { useSettlementListSearchHotkey } from "@/features/supplier-settlements/hooks/use-settlement-list-search-hotkey"
import { outcomeToResult } from "@/features/supplier-settlements/lib/operations"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    DIFF_TYPE_LABEL,
    STATUS_LABEL,
    VIEW_LABEL,
} from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"

function SettlementList({
    urlState,
    patchUrl,
    onOpen,
    returnTo,
}: {
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    onOpen: (statementId: string) => void
    returnTo?: string
}) {
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const [createOpen, setCreateOpen] = React.useState(false)
    const [result, setResult] = React.useState<ResultState>(null)
    const [columnPinning] = React.useState<ColumnPinningState>({
        left: ["statementNo"],
        right: ["actions"],
    })

    React.useEffect(() => {
        setSearchDraft(urlState.q ?? "")
    }, [urlState.q])

    const listQuery = useSettlementListQuery({
        view: urlState.view,
        supplierId: urlState.supplierId,
        periodFrom: urlState.periodFrom,
        periodTo: urlState.periodTo,
        status: urlState.status,
        differenceType: urlState.differenceType,
        q: urlState.q,
        page: urlState.page,
        pageSize: 50,
    })

    const data = listQuery.data
    // D22：分页以 URL 为唯一事实源（page），本地不再持有副本，避免双写漂移；
    // pageSize 固定 50 不入 URL。排序：财务列表不强制加排序（服务端无排序参数），记录在案。
    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, urlState.page - 1),
            pageSize: 50,
        }),
        [urlState.page],
    )

    // P4：清除=清全部筛选参数、view 回 pending（保持原清除语义）、分页回第 1 页；
    // 保留 preview/statementId/returnTo 等导航上下文。空态与工具栏常驻清除共用（D22）。
    const hasActiveFilters = Boolean(
        urlState.supplierId ||
        urlState.periodFrom ||
        urlState.periodTo ||
        urlState.status ||
        urlState.differenceType ||
        urlState.q ||
        urlState.view !== "pending",
    )
    const clearFilters = React.useCallback(() => {
        patchUrl({
            view: "pending",
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            q: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
    }, [patchUrl])

    useSettlementListSearchHotkey()

    const previewRow =
        data?.rows.find((r) => r.statementId === urlState.preview) ?? null

    const columns = useSettlementListColumns(patchUrl, onOpen)

    const canCreate = data?.hasModulePermission && data?.hasDataScope

    if (listQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="API 供应商结算" description="加载失败" />
                <BusinessFailureState
                    title="结算列表加载失败"
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

    const empty = data?.emptyReason

    return (
        <PageScaffold>
            <PageHeader
                title="API 供应商结算"
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.sourceAsOf
                                ? formatDateTime(data.sourceAsOf, "default")
                                : "—"
                        }
                        dateTime={data?.sourceAsOf}
                        label="结算数据更新时间"
                        state={listQuery.isFetching ? "syncing" : "fresh"}
                    />
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-muted-foreground hover:text-foreground"
                            onClick={() => void listQuery.refetch()}
                        >
                            <RefreshCwIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            刷新
                        </Button>
                        <div className="max-sm:hidden">
                            <GuardedBusinessAction
                                type="button"
                                size="sm"
                                disabled={!canCreate}
                                reason={
                                    canCreate
                                        ? undefined
                                        : "当前账号无模块权限或数据范围"
                                }
                                onClick={() => setCreateOpen(true)}
                            >
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建结算草稿
                            </GuardedBusinessAction>
                        </div>
                    </div>
                }
            />

            {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}

            {result ? (
                <FormalActionResult
                    status={
                        result.status === "failed" ? "blocked" : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={
                        result.w12Href ? (
                            <Button
                                type="button"
                                size="sm"
                                render={<Link href={result.w12Href} />}
                            >
                                打开供应商往来应付
                                <ExternalLinkIcon className="size-3.5" />
                            </Button>
                        ) : null
                    }
                />
            ) : null}

            {data?.hasModulePermission && data.hasDataScope ? (
                <MetricStrip columns={4} aria-label="结算快捷筛选">
                    {/* 指标与「差异类型」下拉为双入口（D22 保留）：指标点击时同步清 differenceType，
              避免 status 指标与差异类型组合出矛盾空结果；下拉单独选择差异类型不重置指标。 */}
                    <MetricFilterItem
                        label="待处理"
                        value={data.totals.pendingReconcile}
                        active={urlState.view === "pending" && !urlState.status}
                        onClick={() =>
                            patchUrl({
                                view: "pending",
                                status: undefined,
                                differenceType: undefined,
                                page: 1,
                            })
                        }
                    />
                    <MetricFilterItem
                        label="有差异"
                        value={data.metrics.hasDifference}
                        active={urlState.status === "HAS_DIFFERENCE"}
                        onClick={() =>
                            patchUrl({
                                view: "pending",
                                status: "HAS_DIFFERENCE",
                                differenceType: undefined,
                                page: 1,
                            })
                        }
                    />
                    <MetricFilterItem
                        label="待复核"
                        value={data.metrics.pendingReview}
                        active={urlState.status === "PENDING_REVIEW"}
                        onClick={() =>
                            patchUrl({
                                view: "pending",
                                status: "PENDING_REVIEW",
                                differenceType: undefined,
                                page: 1,
                            })
                        }
                    />
                    <MetricFilterItem
                        label="已确认金额"
                        value={
                            <MoneyValue
                                value={data.metrics.confirmedAmount}
                                taxBasis="gross"
                            />
                        }
                        active={urlState.view === "confirmed"}
                        onClick={() =>
                            patchUrl({
                                view: "confirmed",
                                status: undefined,
                                differenceType: undefined,
                                page: 1,
                            })
                        }
                    />
                </MetricStrip>
            ) : null}

            <Tabs
                value={urlState.view}
                onValueChange={(v) =>
                    patchUrl({
                        view: v as SettlementsUrlState["view"],
                        status: undefined,
                        differenceType: undefined,
                        page: 1,
                    })
                }
            >
                <TabsList>
                    {(
                        Object.keys(VIEW_LABEL) as Array<
                            keyof typeof VIEW_LABEL
                        >
                    ).map((k) => (
                        <TabsTrigger key={k} value={k}>
                            {VIEW_LABEL[k]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <BusinessTableFrame
                title="结算单列表"
                description={data?.filterSummary ?? "默认待处理"}
                toolbar={
                    <ListToolbar
                        search={
                            <div className="flex items-center gap-2">
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        value={searchDraft}
                                        onChange={(e) =>
                                            setSearchDraft(e.target.value)
                                        }
                                        onKeyDown={(e) => {
                                            if (e.key === "Enter") {
                                                patchUrl({
                                                    q:
                                                        searchDraft.trim() ||
                                                        undefined,
                                                    page: 1,
                                                })
                                            }
                                        }}
                                        placeholder="结算单号、外部账单号、供应商"
                                        aria-label="搜索结算单"
                                        data-slot="settlement-list-search"
                                    />
                                </InputGroup>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() =>
                                        patchUrl({
                                            q: searchDraft.trim() || undefined,
                                            page: 1,
                                        })
                                    }
                                >
                                    搜索
                                </Button>
                            </div>
                        }
                        filters={
                            <>
                                <SupplierSearchCombobox
                                    value={urlState.supplierId || undefined}
                                    onValueChange={(id) =>
                                        patchUrl({
                                            supplierId: id || undefined,
                                            page: 1,
                                        })
                                    }
                                    purpose="filter"
                                    className="w-[12rem]"
                                    aria-label="供应商"
                                    placeholder="全部供应商"
                                />
                                <OptionCombobox
                                    value={urlState.status || null}
                                    onValueChange={(v) =>
                                        patchUrl({
                                            status: v || undefined,
                                            page: 1,
                                        })
                                    }
                                    options={[
                                        { value: "", label: "全部状态" },
                                        ...(
                                            Object.keys(STATUS_LABEL) as Array<
                                                keyof typeof STATUS_LABEL
                                            >
                                        ).map((k) => ({
                                            value: k,
                                            label: STATUS_LABEL[k],
                                        })),
                                    ]}
                                    className="w-[9rem]"
                                    size="sm"
                                    aria-label="状态"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.differenceType || null}
                                    onValueChange={(v) =>
                                        patchUrl({
                                            differenceType: (v ||
                                                undefined) as SettlementsUrlState["differenceType"],
                                            page: 1,
                                        })
                                    }
                                    options={[
                                        { value: "", label: "全部差异" },
                                        ...(
                                            Object.keys(
                                                DIFF_TYPE_LABEL,
                                            ) as Array<
                                                keyof typeof DIFF_TYPE_LABEL
                                            >
                                        ).map((k) => ({
                                            value: k,
                                            label: DIFF_TYPE_LABEL[k],
                                        })),
                                    ]}
                                    className="w-[9rem]"
                                    size="sm"
                                    aria-label="差异类型"
                                    allowClear={false}
                                />
                            </>
                        }
                        secondary={
                            <>
                                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                                    期间自
                                    <DatePicker
                                        className="w-[9rem]"
                                        value={urlState.periodFrom || undefined}
                                        onValueChange={(next) =>
                                            patchUrl({
                                                periodFrom: next || undefined,
                                                page: 1,
                                            })
                                        }
                                    />
                                </label>
                                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                                    至
                                    <DatePicker
                                        className="w-[9rem]"
                                        value={urlState.periodTo || undefined}
                                        onValueChange={(next) =>
                                            patchUrl({
                                                periodTo: next || undefined,
                                                page: 1,
                                            })
                                        }
                                    />
                                </label>
                            </>
                        }
                        actions={
                            <div className="flex items-center gap-2">
                                <span
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    共{" "}
                                    {(data?.total ?? 0).toLocaleString("zh-CN")}{" "}
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
                            </div>
                        }
                    />
                }
                table={
                    empty ? (
                        <div className="p-6">
                            {empty === "FILTER_NO_RESULT" ? (
                                <BusinessEmptyState
                                    kind="filter"
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title="当前筛选无结果"
                                    description={`筛选摘要：${data?.filterSummary ?? "—"}。可清除筛选回到默认待处理视图。`}
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
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title="当前范围没有结算单"
                                    description="可选择供应商与期间后重查，或新建结算草稿。"
                                    action={
                                        canCreate ? (
                                            <Button
                                                type="button"
                                                onClick={() =>
                                                    setCreateOpen(true)
                                                }
                                            >
                                                新建结算草稿
                                            </Button>
                                        ) : null
                                    }
                                />
                            )}
                        </div>
                    ) : (
                        <DataTable
                            data={data?.rows ?? []}
                            columns={columns}
                            getRowId={(row) => row.statementId}
                            rowCount={data?.total ?? 0}
                            pagination={pagination}
                            onPaginationChange={(next) => {
                                // D22：只写 URL，分页由 URL 派生，消除本地/URL 双写漂移
                                patchUrl({ page: next.pageIndex + 1 })
                            }}
                            columnPinning={columnPinning}
                            enableColumnPinning
                            manualPagination
                            layout="flush"
                            density="compact"
                            onRowPreview={(row) =>
                                patchUrl({ preview: row.statementId })
                            }
                            onRowOpen={(row) => onOpen(row.statementId)}
                        />
                    )
                }
            />

            <QuickPreviewSheet
                open={Boolean(urlState.preview)}
                onOpenChange={(open) => {
                    if (!open) patchUrl({ preview: undefined })
                }}
                size="detail"
                title={previewRow?.statementNo ?? "结算预览"}
                description={
                    previewRow
                        ? `${previewRow.supplierName} · ${previewRow.periodLabel}`
                        : undefined
                }
            >
                {previewRow ? (
                    <div className="space-y-4 p-1">
                        <DocumentTotals
                            title="金额摘要（含税）"
                            items={[
                                {
                                    id: "erp",
                                    label: "ERP 计算金额",
                                    value: (
                                        <MoneyValue
                                            value={previewRow.erpAmountGross}
                                            taxBasis="gross"
                                        />
                                    ),
                                    basis: "含税",
                                },
                                {
                                    id: "bill",
                                    label: "供应商账单金额",
                                    value: previewRow.supplierAmountGross ? (
                                        <MoneyValue
                                            value={
                                                previewRow.supplierAmountGross
                                            }
                                            taxBasis="gross"
                                        />
                                    ) : (
                                        "账单未同步"
                                    ),
                                    basis: "含税",
                                },
                                {
                                    id: "diff",
                                    label: "差异",
                                    value: previewRow.differenceAmountGross ? (
                                        <MoneyValue
                                            value={
                                                previewRow.differenceAmountGross
                                            }
                                            taxBasis="gross"
                                        />
                                    ) : (
                                        "—"
                                    ),
                                    warning:
                                        previewRow.differenceDirectionLabel,
                                },
                            ]}
                        />
                        <div className="flex flex-wrap gap-2 text-sm text-muted-foreground">
                            <span>
                                经办 {previewRow.preparedByLabel} · 复核{" "}
                                {previewRow.reviewedByLabel}
                            </span>
                            <BusinessStatusBadge
                                context="list"
                                label={previewRow.statusLabel}
                                tone={previewRow.statusTone}
                            />
                        </div>
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                onClick={() => onOpen(previewRow.statementId)}
                            >
                                查看详情
                            </Button>
                            {previewRow.unresolvedDifferenceCount > 0 ? (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    onClick={() =>
                                        patchUrl({
                                            statementId: previewRow.statementId,
                                            section: "differences",
                                            preview: undefined,
                                        })
                                    }
                                >
                                    打开差异处理
                                </Button>
                            ) : null}
                        </div>
                        <p className="text-xs text-muted-foreground">
                            键盘：列表 Enter
                            打开预览；详情页可继续提交复核并查询处理结果。
                        </p>
                    </div>
                ) : (
                    <div className="flex flex-col items-start gap-3 p-5">
                        <p className="text-sm text-muted-foreground">
                            未找到预览行，可能已被移出当前筛选范围。
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => patchUrl({ preview: undefined })}
                        >
                            关闭预览
                        </Button>
                    </div>
                )}
            </QuickPreviewSheet>

            <CreateDraftDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
                onCreated={(outcome) => {
                    setResult(outcomeToResult(outcome))
                    if (outcome.status === "succeeded" && outcome.statementId) {
                        onOpen(outcome.statementId)
                    }
                }}
            />
        </PageScaffold>
    )
}

export { SettlementList }
