"use client"

import * as React from "react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type {
    ReceivableScopeListView,
    ReceivableScopeQuery,
} from "@/features/customer-receivables/api/scoped"
import {
    createScopedInvoiceColumns,
    createScopedReceiptColumns,
    createScopedReceivableColumns,
} from "@/features/customer-receivables/components/scoped-columns"
import {
    ReceivableScopeEmptyState,
    ReceivableScopeSummary,
} from "@/features/customer-receivables/components/scoped-summary"
import { ReceivableScopeToolbar } from "@/features/customer-receivables/pages/components/receivable-scope-toolbar"
import { VIEW_LABEL } from "@/features/customer-receivables/types"
import type { useReceivableScopeUrlState } from "@/features/customer-receivables/pages/hooks/use-receivable-scope-url-state"
import { getErrorMessage } from "@/lib/api/errors"

const SCOPE_VIEWS = ["receivable", "receipt", "sales_invoice"] as const

type Props = {
    urlState: ReturnType<typeof useReceivableScopeUrlState>
    data: ReceivableScopeListView | undefined
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    onExport: () => void
    exporting: boolean
    onPreview: (kind: "receivable" | "receipt" | "invoice", id: string) => void
    previewId: string | undefined
}

/** 客户往来范围列表页：范围筛选、汇总、列表三分态与版本绑定导出。 */
export function ReceivableScopeListPane({
    urlState,
    data,
    isPending,
    isError,
    error,
    onRetry,
    onExport,
    exporting,
    onPreview,
    previewId,
}: Props) {
    const receivableColumns = React.useMemo(createScopedReceivableColumns, [])
    const receiptColumns = React.useMemo(createScopedReceiptColumns, [])
    const invoiceColumns = React.useMemo(createScopedInvoiceColumns, [])

    const receivableRows =
        urlState.view === "receivable" ? [...(data?.receivables ?? [])] : []
    const receiptRows =
        urlState.view === "receipt" ? [...(data?.receipts ?? [])] : []
    const invoiceRows =
        urlState.view !== "receivable" && urlState.view !== "receipt"
            ? [...(data?.invoices ?? [])]
            : []

    const table =
        isError && !data ? (
            <BusinessFailureState
                title="客户往来范围加载失败"
                error={error}
                action={
                    <Button
                        id="customer-receivables-scope-list-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                }
            />
        ) : isPending && !data ? (
            <div className="h-64 animate-pulse rounded-xl bg-muted" />
        ) : data && (data.emptyReason === "no_scope" || !data.hasScope) ? (
            <BusinessEmptyState
                kind="no-scope"
                title="当前角色未配置客户往来范围"
                description="不得用 0 元假装无往来。请申请数据范围后再查询。"
                className={listWorkspaceEmptyStateClassName}
            />
        ) : data && data.total === 0 ? (
            <BusinessEmptyState
                kind="filter"
                title="无匹配往来记录"
                description="无匹配记录，可清除筛选后重试。"
                className={listWorkspaceEmptyStateClassName}
                action={
                    <Button
                        id="customer-receivables-scope-empty-clear-filters"
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={urlState.clearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        ) : urlState.view === "receivable" ? (
            <DataTable
                id="customer-receivables-scope-list-receivable"
                data={receivableRows}
                columns={receivableColumns}
                getRowId={(row) => row.id}
                onRowPreview={(row) => onPreview("receivable", row.id)}
                highlightedRowId={previewId}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        ) : urlState.view === "receipt" ? (
            <DataTable
                id="customer-receivables-scope-list-receipt"
                data={receiptRows}
                columns={receiptColumns}
                getRowId={(row) => row.id}
                onRowPreview={(row) => onPreview("receipt", row.id)}
                highlightedRowId={previewId}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        ) : (
            <DataTable
                id="customer-receivables-scope-list-invoice"
                data={invoiceRows}
                columns={invoiceColumns}
                getRowId={(row) => row.id}
                onRowPreview={(row) => onPreview("invoice", row.id)}
                highlightedRowId={previewId}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        )

    return (
        <>
            <Tabs
                value={urlState.view}
                onValueChange={(nextView) => {
                    urlState.changeView(
                        nextView as ReceivableScopeQuery["view"],
                    )
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
                            id={`customer-receivables-scope-view-${item}`}
                        >
                            {VIEW_LABEL[item as keyof typeof VIEW_LABEL] ??
                                item}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <ReceivableScopeSummary data={data} />

            {data && data.total === 0 ? (
                <ReceivableScopeEmptyState
                    data={data}
                    hasFilters={urlState.hasActiveFilters}
                    onClearFilters={urlState.clearFilters}
                    clearId="customer-receivables-scope-banner-clear-filters"
                />
            ) : null}

            {isError && data ? (
                <Alert variant="destructive">
                    <AlertTitle>刷新失败，当前显示的是上次成功数据</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(error, "请稍后重试。")}
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
                    <ReceivableScopeToolbar
                        view={urlState.view}
                        searchDraft={urlState.searchDraft}
                        setSearchDraft={urlState.setSearchDraft}
                        searchInputRef={urlState.searchInputRef}
                        salesOwnerDraft={urlState.salesOwnerDraft}
                        setSalesOwnerDraft={urlState.setSalesOwnerDraft}
                        operatorDraft={urlState.operatorDraft}
                        setOperatorDraft={urlState.setOperatorDraft}
                        operatorKindDraft={urlState.operatorKindDraft}
                        setOperatorKindDraft={urlState.setOperatorKindDraft}
                        orgDraft={urlState.orgDraft}
                        setOrgDraft={urlState.setOrgDraft}
                        descendantsDraft={urlState.descendantsDraft}
                        setDescendantsDraft={urlState.setDescendantsDraft}
                        panelOpen={urlState.panelOpen}
                        setPanelOpen={urlState.setPanelOpen}
                        appliedChips={urlState.appliedChips}
                        removeFilter={urlState.removeFilter}
                        applyFilters={urlState.applyFilters}
                        resetMoreFilters={() => {
                            urlState.setSalesOwnerDraft("")
                            urlState.setOperatorDraft("")
                            urlState.setOperatorKindDraft("")
                            urlState.setOrgDraft("")
                            urlState.setDescendantsDraft(false)
                        }}
                        clearFilters={urlState.clearFilters}
                        hasPendingChanges={urlState.hasPendingChanges}
                        ownerOptions={data?.ownerOptions ?? []}
                        resultCount={data?.total}
                        loading={isPending}
                        failed={isError}
                    />
                }
                table={table}
            />

            <div className="flex min-w-0 flex-wrap items-center justify-end gap-2">
                <Button
                    id="customer-receivables-scope-export"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={exporting || !data || data.total === 0}
                    onClick={onExport}
                >
                    {exporting ? "导出中…" : "导出当前范围"}
                </Button>
            </div>
        </>
    )
}

export function ReceivableScopeViewsNav({
    onOpenLegacy,
}: {
    onOpenLegacy: () => void
}) {
    return (
        <ListWorkSurface
            ariaLabel="客户往来范围"
            toolbarClassName="pt-3 pb-2"
            views={
                <ListWorkspaceViews
                    ariaLabel="客户往来范围工作视图"
                    items={[
                        {
                            id: "customer-receivables-scope-nav",
                            label: "按数据范围查询",
                            active: true,
                            onClick: () => undefined,
                        },
                        {
                            id: "customer-receivables-legacy-nav",
                            label: "全部往来视图",
                            active: false,
                            onClick: onOpenLegacy,
                        },
                    ]}
                />
            }
            toolbar={null}
            table={null}
        />
    )
}
