"use client"

import * as React from "react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    FundsScopeBanner,
    MoneyValue,
} from "@/components/business"
import {
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { ListSearchField } from "@/components/business/list-search-field"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { PageScaffold } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { QuickPreviewSheet } from "@/components/business"
import {
    PreviewAmount,
    PreviewFact,
    PreviewNote,
    PreviewSection,
} from "@/components/business/financial-preview"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import type { SupplierScopeListView } from "@/features/supplier-payables/api/scoped"
import { exportSupplierScopeCsv } from "@/features/supplier-payables/lib/scoped-export"
import {
    useSupplierScopeDetailQuery,
    useSupplierScopeListQuery,
} from "@/features/supplier-payables/hooks/scoped-queries"
import { useSupplierScopeUrlState } from "@/features/supplier-payables/pages/hooks/use-supplier-scope-url-state"
import {
    createSupplierScopedAllocationColumns,
    createSupplierScopedPayableColumns,
    createSupplierScopedPaymentColumns,
} from "@/features/supplier-payables/components/scoped-columns"
import { VIEW_LABEL } from "@/features/supplier-payables/types"
import { downloadListCsv } from "@/lib/list-export"
import { getErrorMessage } from "@/lib/api/errors"
import { scopeText } from "@/lib/ui-text"
import { toast } from "@/components/ui/toast"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

const SCOPE_VIEWS = ["payable", "payment", "purchase_invoice"] as const
const toolbarPrefix = "supplier-payables-scope-toolbar"

/** 供应商往来范围页：采购负责人/付款/收票经办人分别查询，跨页绑定范围版本。 */
export function SupplierScopePage() {
    const urlState = useSupplierScopeUrlState()
    const listQuery = useSupplierScopeListQuery(urlState.query)
    const [preview, setPreview] = React.useState<{
        kind: "payable" | "payment"
        id: string
    } | null>(null)
    const detailQuery = useSupplierScopeDetailQuery(
        preview?.kind ?? null,
        preview?.id ?? null,
    )
    const [exporting, setExporting] = React.useState(false)

    const payableColumns = React.useMemo(createSupplierScopedPayableColumns, [])
    const paymentColumns = React.useMemo(createSupplierScopedPaymentColumns, [])
    const allocationColumns = React.useMemo(
        createSupplierScopedAllocationColumns,
        [],
    )
    const data: SupplierScopeListView | undefined = listQuery.data

    async function handleExport() {
        if (exporting) return
        setExporting(true)
        try {
            const result = await exportSupplierScopeCsv(urlState.query)
            downloadListCsv(result.content, result.fileName)
            toast.add({
                title: "导出已完成",
                description: `已按当前范围生成 ${result.fileName}（${result.rowCount} 条）。`,
                type: "success",
            })
        } catch (error) {
            toast.add({
                title: "导出失败",
                description: getErrorMessage(error, "请重新查询后重试"),
                type: "error",
            })
        } finally {
            setExporting(false)
        }
    }

    const payableRows =
        urlState.view === "payable" ? [...(data?.payables ?? [])] : []
    const paymentRows =
        urlState.view === "payment" ? [...(data?.payments ?? [])] : []
    const allocationRows =
        urlState.view !== "payable" && urlState.view !== "payment"
            ? [...(data?.allocations ?? [])]
            : []

    const table =
        listQuery.isError && !data ? (
            <BusinessFailureState
                title="供应商往来范围加载失败"
                error={listQuery.error}
                action={
                    <Button
                        id="supplier-payables-scope-list-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void listQuery.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        ) : listQuery.isPending && !data ? (
            <div className="h-64 animate-pulse rounded-xl bg-muted" />
        ) : data && (data.emptyReason === "no_scope" || !data.hasScope) ? (
            <BusinessEmptyState
                kind="no-scope"
                title="当前角色未配置供应商往来范围"
                description="不能显示为 0 元应付。请联系管理员配置范围后再查询。"
            />
        ) : data && data.total === 0 ? (
            <BusinessEmptyState
                kind="filter"
                title="无匹配往来记录"
                description="无匹配记录，可清除筛选后重试。"
                action={
                    <Button
                        id="supplier-payables-scope-empty-clear-filters"
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={urlState.clearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        ) : urlState.view === "payable" ? (
            <DataTable
                id="supplier-payables-scope-list-payable"
                data={payableRows}
                columns={payableColumns}
                getRowId={(row) => row.id}
                onRowPreview={(row) =>
                    setPreview({ kind: "payable", id: row.id })
                }
                highlightedRowId={preview?.id}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        ) : urlState.view === "payment" ? (
            <DataTable
                id="supplier-payables-scope-list-payment"
                data={paymentRows}
                columns={paymentColumns}
                getRowId={(row) => row.id}
                onRowPreview={(row) =>
                    setPreview({ kind: "payment", id: row.id })
                }
                highlightedRowId={preview?.id}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        ) : (
            <DataTable
                id="supplier-payables-scope-list-allocation"
                data={allocationRows}
                columns={allocationColumns}
                getRowId={(row) => row.id}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        )

    const detail = detailQuery.data
    const limited =
        detail?.kind === "payable"
            ? detail.payable.permission_limited
            : detail?.kind === "payment"
              ? detail.payment.permission_limited
              : false

    return (
        <PageScaffold density="compact" className="space-y-4">
            <ListWorkspaceHeader
                eyebrow="财务"
                title="供应商往来（按数据范围）"
                description="按采购负责人与付款/收票经办人查询；部分受限仅显示获授权份额。"
            />

            <Tabs
                value={urlState.view}
                onValueChange={(nextView) => {
                    urlState.changeView(nextView as typeof urlState.view)
                }}
            >
                <TabsList
                    variant="line"
                    className="w-full overflow-x-auto border-b border-border"
                >
                    {SCOPE_VIEWS.map((item) => (
                        <TabsTrigger
                            key={item}
                            value={item}
                            id={`supplier-payables-scope-view-${item}`}
                        >
                            {VIEW_LABEL[item as keyof typeof VIEW_LABEL] ??
                                item}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <FundsScopeBanner
                scopeSummary={data?.scopeSummary}
                asOf={data?.asOf}
                permissionLimited={
                    data?.summary?.permission_limited ||
                    data?.payables.some((row) => row.permission_limited) ||
                    data?.payments.some((row) => row.permission_limited) ||
                    data?.allocations.some((row) => row.permission_limited) ||
                    false
                }
                unassigned={data?.summary?.unassigned}
            />

            {data?.summary ? (
                <div className="grid min-w-0 gap-3 sm:grid-cols-3">
                    <div className="min-w-0 rounded-lg border p-3">
                        <div className="text-xs text-muted-foreground">
                            整单合计
                        </div>
                        <div className="mt-1">
                            <MoneyValue
                                value={data.summary.whole_total}
                                unavailableReason={
                                    data.summary.whole_total == null
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </div>
                    </div>
                    <div className="min-w-0 rounded-lg border p-3">
                        <div className="text-xs text-muted-foreground">
                            {scopeText.unassignedShare}
                        </div>
                        <div className="mt-1">
                            <MoneyValue value={data.summary.unassigned} />
                        </div>
                    </div>
                    <div className="min-w-0 rounded-lg border p-3">
                        <div className="text-xs text-muted-foreground">
                            匹配分组（{data.summary.grouped.length}）
                        </div>
                        <ul className="mt-1 min-w-0 space-y-1">
                            {data.summary.grouped.length === 0 ? (
                                <li className="text-xs text-muted-foreground">
                                    暂无匹配份额
                                </li>
                            ) : (
                                data.summary.grouped
                                    .slice(0, 5)
                                    .map((share) => (
                                        <li
                                            key={share.owner_user_id}
                                            className="flex min-w-0 items-baseline justify-between gap-2 text-xs"
                                        >
                                            <span className="min-w-0 break-words">
                                                {share.owner_user_id}
                                            </span>
                                            <MoneyValue
                                                value={share.visible_share}
                                                className="shrink-0"
                                            />
                                        </li>
                                    ))
                            )}
                        </ul>
                    </div>
                </div>
            ) : null}

            {listQuery.isError && data ? (
                <Alert variant="destructive">
                    <AlertTitle>刷新失败，当前显示的是上次成功数据</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(listQuery.error, "请稍后重试。")}
                    </AlertDescription>
                </Alert>
            ) : null}

            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex min-w-0 flex-wrap items-baseline gap-2">
                        按数据范围查询
                        <span
                            aria-live="polite"
                            className="font-normal text-muted-foreground"
                        >
                            {(data?.total ?? 0).toLocaleString("zh-CN")} 条
                        </span>
                    </span>
                }
                description={
                    <span aria-live="polite" className="break-words">
                        {data?.scopeSummary ?? "加载中…"}
                    </span>
                }
                toolbar={
                    <ListWorkspaceFilterBar
                        density="compact"
                        idPrefix={toolbarPrefix}
                        formAriaLabel="供应商往来范围查询"
                        onSubmit={urlState.applyFilters}
                        queryButtonId={`${toolbarPrefix}-apply`}
                        moreButtonId={`${toolbarPrefix}-more-filters`}
                        clearButtonId={`${toolbarPrefix}-clear-all`}
                        search={
                            <ListSearchField
                                id={`${toolbarPrefix}-search`}
                                searchInputRef={urlState.searchInputRef}
                                value={urlState.searchDraft}
                                onChange={urlState.setSearchDraft}
                                placeholder="供应商、采购单、付款单"
                                aria-label="搜索供应商往来范围"
                            />
                        }
                        moreCount={urlState.appliedChips.length}
                        moreOpen={urlState.panelOpen}
                        onToggleMore={() =>
                            urlState.setPanelOpen((open) => !open)
                        }
                        morePanelId={`${toolbarPrefix}-more-panel`}
                        morePanelAriaLabel="供应商往来范围更多筛选条件"
                        onResetMore={() => {
                            urlState.setProcurementOwnerDraft("")
                            urlState.setOperatorDraft("")
                            urlState.setOrgDraft("")
                            urlState.setDescendantsDraft(false)
                        }}
                        morePanel={
                            <div className="grid min-w-0 gap-5">
                                <ResponsibleUserFilter
                                    id={`${toolbarPrefix}-procurement-owner`}
                                    label="采购负责人"
                                    value={urlState.procurementOwnerDraft}
                                    onChange={urlState.setProcurementOwnerDraft}
                                    options={data?.ownerOptions ?? []}
                                />
                                <ResponsibleUserFilter
                                    id={`${toolbarPrefix}-operator`}
                                    label={
                                        urlState.view === "payment"
                                            ? "付款经办人"
                                            : urlState.view ===
                                                "purchase_invoice"
                                              ? "收票经办人"
                                              : "经办人"
                                    }
                                    value={urlState.operatorDraft}
                                    onChange={urlState.setOperatorDraft}
                                    options={data?.ownerOptions ?? []}
                                />
                                <ListWorkspaceFilterField
                                    htmlFor={`${toolbarPrefix}-org`}
                                    label="业务组织（逗号分隔组织 ID）"
                                >
                                    <input
                                        id={`${toolbarPrefix}-org`}
                                        className="h-9 w-full min-w-0 rounded-md border bg-background px-3 text-sm"
                                        value={urlState.orgDraft}
                                        onChange={(event) =>
                                            urlState.setOrgDraft(
                                                event.target.value,
                                            )
                                        }
                                        placeholder="全部组织"
                                        aria-label="筛选业务组织"
                                    />
                                </ListWorkspaceFilterField>
                                <label
                                    htmlFor={`${toolbarPrefix}-include-descendants`}
                                    className="flex min-w-0 items-center gap-2 text-sm"
                                >
                                    <input
                                        id={`${toolbarPrefix}-include-descendants`}
                                        type="checkbox"
                                        checked={urlState.descendantsDraft}
                                        onChange={(event) =>
                                            urlState.setDescendantsDraft(
                                                event.target.checked,
                                            )
                                        }
                                    />
                                    包含下级组织
                                </label>
                            </div>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: listQuery.isPending,
                            failed: listQuery.isError,
                            resultCount: data?.total,
                            noun: "条往来",
                            loadingLabel: "正在加载往来…",
                        })}
                        chips={urlState.appliedChips}
                        onClearChip={(key) =>
                            urlState.removeFilter(
                                key as (typeof urlState.appliedChips)[number]["key"],
                            )
                        }
                        onClearAll={urlState.clearFilters}
                        hasPendingChanges={urlState.hasPendingChanges}
                        pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                    />
                }
                table={table}
            />

            <div className="flex min-w-0 flex-wrap items-center justify-end gap-2">
                <Button
                    id="supplier-payables-scope-export"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={exporting || !data || data.total === 0}
                    onClick={() => void handleExport()}
                >
                    {exporting ? "导出中…" : "导出当前范围"}
                </Button>
            </div>

            <QuickPreviewSheet
                id="supplier-payables-scope-preview-sheet"
                open={preview != null}
                onOpenChange={(nextOpen) => {
                    if (!nextOpen) setPreview(null)
                }}
                size="preview"
                contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px]"
                title={
                    detail?.kind === "payable"
                        ? `应付子账 ${detail.payable.id}`
                        : detail?.kind === "payment"
                          ? `付款单 ${detail.payment.payment_no}`
                          : "往来详情"
                }
                footer={
                    <Button
                        id="supplier-payables-scope-preview-close"
                        type="button"
                        variant="outline"
                        onClick={() => setPreview(null)}
                    >
                        关闭
                    </Button>
                }
            >
                {detailQuery.isPending ? (
                    <div className="space-y-3 px-7 py-6">
                        <div className="h-24 animate-pulse rounded-xl bg-muted" />
                    </div>
                ) : detailQuery.isError ? (
                    <div className="space-y-3 px-7 py-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                detailQuery.error,
                                "详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            id="supplier-payables-scope-preview-retry"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : detail?.kind === "payable" ? (
                    <div className="min-h-0 flex-1 space-y-6 overflow-auto px-7 py-6 text-sm">
                        <PreviewAmount
                            label="获授权已核销"
                            value={detail.payable.visible_settled_share}
                        >
                            <span>
                                整单金额{" "}
                                <MoneyValue
                                    value={detail.payable.gross_total}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                            <span>
                                未分配{" "}
                                <MoneyValue
                                    value={detail.payable.open_total}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                        </PreviewAmount>
                        <PreviewSection title="来源">
                            <dl className="space-y-3">
                                <PreviewFact label="来源单据">
                                    <span className="num break-all">
                                        {detail.payable.source_document_id}
                                    </span>
                                </PreviewFact>
                            </dl>
                        </PreviewSection>
                        {limited ? (
                            <PreviewNote>
                                {scopeText.limitedOnlyVisibleShare}
                            </PreviewNote>
                        ) : null}
                    </div>
                ) : detail?.kind === "payment" ? (
                    <div className="min-h-0 flex-1 space-y-6 overflow-auto px-7 py-6 text-sm">
                        <PreviewAmount
                            label="获授权已分配"
                            value={detail.payment.visible_allocated_share}
                        >
                            <span>
                                整单金额{" "}
                                <MoneyValue
                                    value={detail.payment.amount}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                            <span>
                                未分配{" "}
                                <MoneyValue
                                    value={detail.payment.unallocated_amount}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                        </PreviewAmount>
                        {limited ? (
                            <PreviewNote>
                                {scopeText.limitedOnlyVisibleShare}
                            </PreviewNote>
                        ) : null}
                    </div>
                ) : (
                    <div className="px-7 py-6 text-sm text-muted-foreground">
                        未找到该笔记录，可能已超出当前数据范围。
                    </div>
                )}
            </QuickPreviewSheet>
        </PageScaffold>
    )
}
