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
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { PageScaffold } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import type { ColumnDef } from "@tanstack/react-table"
import { downloadListCsv } from "@/lib/list-export"
import { getErrorMessage } from "@/lib/api/errors"
import { toast } from "@/components/ui/toast"
import type { ScopedInvoiceRequestWire } from "@/features/invoice-requests/scoped"
import { exportInvoiceRequestScopeCsv } from "@/features/invoice-requests/scoped-export"
import {
    useInvoiceRequestScopeDetailQuery,
    useInvoiceRequestScopeListQuery,
} from "@/features/invoice-requests/hooks/scoped-queries"
import { useInvoiceRequestScopeUrlState } from "@/features/invoice-requests/hooks/use-scope-url-state"
import { scopeText } from "@/lib/ui-text"

const toolbarPrefix = "invoice-request-scope-toolbar"

function ScopeColumns(): ColumnDef<ScopedInvoiceRequestWire>[] {
    return [
        {
            id: "doc",
            header: "申请单号",
            meta: { label: "申请单号", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <div className="num break-words text-sm font-medium">
                        {row.original.request_no}
                    </div>
                    <div className="num text-xs text-muted-foreground">
                        {row.original.sales_order_no}
                    </div>
                </div>
            ),
        },
        {
            id: "amount",
            header: "申请金额",
            meta: {
                label: "申请金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => <MoneyValue value={row.original.amount} />,
        },
        {
            id: "people",
            header: "申请人 / 当前处理人",
            meta: { label: "经办", width: "default" },
            cell: ({ row }) => (
                <div className="flex min-w-0 flex-col gap-1 text-xs">
                    <span className="break-words">
                        申请人 {row.original.applicant_user_id}
                    </span>
                    <span className="break-words text-muted-foreground">
                        {row.original.handler_user_id
                            ? `当前处理人 ${row.original.handler_user_id}`
                            : "暂无当前处理人"}
                    </span>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            cell: ({ row }) => (
                <span className="text-sm">{row.original.status}</span>
            ),
        },
    ]
}

/** 开票申请范围页：负责销售/申请人/当前开票处理人分别查询，不改变开票准入。 */
export function InvoiceRequestScopePage() {
    const urlState = useInvoiceRequestScopeUrlState()
    const listQuery = useInvoiceRequestScopeListQuery(urlState.query)
    const [detailId, setDetailId] = React.useState<string | null>(null)
    const detailQuery = useInvoiceRequestScopeDetailQuery(detailId)
    const [exporting, setExporting] = React.useState(false)
    const columns = React.useMemo(ScopeColumns, [])
    const data = listQuery.data

    async function handleExport() {
        if (exporting) return
        setExporting(true)
        try {
            const result = await exportInvoiceRequestScopeCsv(urlState.query)
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

    const table =
        listQuery.isError && !data ? (
            <BusinessFailureState
                title="开票申请范围加载失败"
                error={listQuery.error}
                action={
                    <Button
                        id="invoice-request-scope-list-retry"
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
                title="当前角色未配置开票申请范围"
                description="不得用 0 元假装无申请。请申请数据范围后再查询。"
            />
        ) : data && data.total === 0 ? (
            <BusinessEmptyState
                kind="filter"
                title="无匹配开票申请"
                description="无匹配记录，可清除筛选后重试。"
                action={
                    <Button
                        id="invoice-request-scope-empty-clear-filters"
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={urlState.clearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        ) : (
            <DataTable
                id="invoice-request-scope-list"
                data={[...(data?.requests ?? [])]}
                columns={columns}
                getRowId={(row) => row.id}
                onRowPreview={(row) => setDetailId(row.id)}
                highlightedRowId={detailId ?? undefined}
                rowCount={data?.total ?? 0}
                pagination={urlState.pagination}
                onPaginationChange={(next) =>
                    urlState.handlePaginationChange(next, data?.scopeVersion)
                }
                layout="flush"
            />
        )

    return (
        <PageScaffold density="compact" className="space-y-4">
            <ListWorkspaceHeader
                eyebrow="财务"
                title="开票申请（按数据范围）"
                description="按负责销售、申请人与当前开票处理人查询；不改变正式开票准入。"
            />

            <FundsScopeBanner
                scopeSummary={data?.scopeSummary}
                asOf={data?.asOf}
                permissionLimited={
                    data?.requests.some((row) => row.permission_limited) ??
                    false
                }
                unassigned={data?.summary?.unassigned}
            />

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
                        formAriaLabel="开票申请范围查询"
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
                                placeholder="申请单号、抬头、事由"
                                aria-label="搜索开票申请范围"
                            />
                        }
                        moreCount={urlState.appliedChips.length}
                        moreOpen={urlState.panelOpen}
                        onToggleMore={() =>
                            urlState.setPanelOpen((open) => !open)
                        }
                        morePanelId={`${toolbarPrefix}-more-panel`}
                        morePanelAriaLabel="开票申请范围更多筛选条件"
                        onResetMore={() => {
                            urlState.setSalesOwnerDraft("")
                            urlState.setApplicantDraft("")
                            urlState.setHandlerDraft("")
                            urlState.setOrgDraft("")
                            urlState.setDescendantsDraft(false)
                        }}
                        morePanel={
                            <div className="grid min-w-0 gap-5">
                                <ResponsibleUserFilter
                                    id={`${toolbarPrefix}-sales-owner`}
                                    label="负责销售"
                                    value={urlState.salesOwnerDraft}
                                    onChange={urlState.setSalesOwnerDraft}
                                    options={data?.ownerOptions ?? []}
                                />
                                <ResponsibleUserFilter
                                    id={`${toolbarPrefix}-applicant`}
                                    label="申请人"
                                    value={urlState.applicantDraft}
                                    onChange={urlState.setApplicantDraft}
                                    options={data?.ownerOptions ?? []}
                                />
                                <ResponsibleUserFilter
                                    id={`${toolbarPrefix}-handler`}
                                    label="当前开票处理人"
                                    value={urlState.handlerDraft}
                                    onChange={urlState.setHandlerDraft}
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
                            noun: "条申请",
                            loadingLabel: "正在加载申请…",
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
                    id="invoice-request-scope-export"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={exporting || !data || data.total === 0}
                    onClick={() => void handleExport()}
                >
                    {exporting ? "导出中…" : "导出当前范围"}
                </Button>
            </div>

            {detailId ? (
                <section
                    aria-label="开票申请范围详情"
                    className="min-w-0 rounded-xl border p-4"
                >
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                        <h2 className="min-w-0 break-words text-sm font-semibold">
                            申请详情
                        </h2>
                        <Button
                            id="invoice-request-scope-detail-close"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setDetailId(null)}
                        >
                            关闭
                        </Button>
                    </div>
                    {detailQuery.isPending ? (
                        <p className="mt-2 text-sm text-muted-foreground">
                            正在读取申请详情…
                        </p>
                    ) : detailQuery.isError ? (
                        <div role="alert" className="mt-2 space-y-2">
                            <p className="text-sm text-muted-foreground">
                                {getErrorMessage(
                                    detailQuery.error,
                                    "申请详情读取失败",
                                )}
                            </p>
                            <Button
                                id="invoice-request-scope-detail-retry"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        </div>
                    ) : detailQuery.data ? (
                        <div className="mt-2 overflow-x-auto">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        {[
                                            "申请单号",
                                            "销售单",
                                            "申请金额",
                                            "申请人",
                                            "当前处理人",
                                            "状态",
                                        ].map((label) => (
                                            <TableHead key={label}>
                                                {label}
                                            </TableHead>
                                        ))}
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    <TableRow>
                                        <TableCell>
                                            {detailQuery.data.request_no}
                                        </TableCell>
                                        <TableCell>
                                            {detailQuery.data.sales_order_no}
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={detailQuery.data.amount}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            {detailQuery.data.applicant_user_id}
                                        </TableCell>
                                        <TableCell>
                                            {detailQuery.data.handler_user_id ??
                                                "—"}
                                        </TableCell>
                                        <TableCell>
                                            {detailQuery.data.status}
                                        </TableCell>
                                    </TableRow>
                                </TableBody>
                            </Table>
                            {detailQuery.data.permission_limited ? (
                                <p className="mt-2 text-xs text-muted-foreground">
                                    {scopeText.limitedOnlyVisibleShare}
                                    。申请金额为行级事实始终返回。
                                </p>
                            ) : null}
                        </div>
                    ) : (
                        <p className="mt-2 text-sm text-muted-foreground">
                            未找到该申请，可能已超出当前数据范围。
                        </p>
                    )}
                </section>
            ) : null}
        </PageScaffold>
    )
}
