"use client"

import { useCallback, useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    ListDataTable,
    ListWorkspacePage,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    listWorkspaceEmptyStateClassName,
    type ColumnDef,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import { apiGet } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import type { BackgroundJobView } from "../api"
import { MyTransferPreview } from "../components/my-transfer-preview"
import {
    JOB_STATUS_LABELS,
    backgroundJobIdentity,
    formatJobDateTime,
} from "../labels"

const TITLE = "我的文件任务"
const PAGE_SIZE = 20
const EMPTY_ROWS: BackgroundJobView[] = []
const STATUS_TONES: Record<BackgroundJobView["status"], StatusTone> = {
    pending: "neutral",
    running: "info",
    partially_succeeded: "warning",
    succeeded: "success",
    failed: "destructive",
    cancelled: "neutral",
}

function transferLabel(job: BackgroundJobView) {
    const { title, number } = backgroundJobIdentity(job)
    return `${title}（${number}）`
}

function transferColumns(
    openPreview: (job: BackgroundJobView) => void,
): ColumnDef<BackgroundJobView>[] {
    return [
        {
            id: "job",
            header: "文件任务",
            meta: { width: "flex" },
            cell: ({ row }) => {
                const { title, number } = backgroundJobIdentity(row.original)
                return (
                    <TableIdentityCell
                        primary={title}
                        primaryTitle={title}
                        secondary={number}
                        secondaryTitle={number}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "status",
            header: "状态",
            meta: { width: "status" },
            cell: ({ row }) => {
                const { status } = row.original
                return (
                    <BusinessStatusBadge
                        context="list"
                        tone={STATUS_TONES[status]}
                        label={JOB_STATUS_LABELS[status]}
                        appearance={status === "succeeded" ? "quiet" : "badge"}
                    />
                )
            },
        },
        {
            id: "created",
            header: "创建时间",
            meta: { width: "default", numeric: true },
            cell: ({ row }) => (
                <TableTextCell
                    value={formatJobDateTime(row.original.created_at)}
                />
            ),
        },
        {
            id: "error",
            header: "异常说明",
            meta: { width: "reference" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.error_summary}
                    clamp={2}
                    className="wrap-anywhere whitespace-normal text-destructive"
                />
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { width: "default", align: "end" },
            cell: ({ row }) => {
                const suffix = toAutomationIdSegment(row.original.id)
                return (
                    <TableRowActions
                        moreId={`my-transfers-row-${suffix}-more`}
                        moreLabel={`${transferLabel(row.original)} 更多操作`}
                        actions={[
                            {
                                id: `my-transfers-open-${suffix}`,
                                label: "查看结果与后续操作",
                                onClick: () => openPreview(row.original),
                            },
                        ]}
                    />
                )
            },
        },
    ]
}

// The shared pager derives different IDs and exposes a page-size control.
function TransferPagination({
    page,
    total,
    onPageChange,
}: {
    page: number
    total: number
    onPageChange: (page: number) => void
}) {
    return (
        <div
            data-slot="data-table-pagination"
            className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2"
        >
            <span className="num text-muted-foreground">
                共 {total.toLocaleString("zh-CN")} 条
            </span>
            <div
                className="flex flex-wrap items-center gap-2"
                role="group"
                aria-label="翻页"
            >
                <span className="num text-muted-foreground">第 {page} 页</span>
                <Button
                    id="my-transfers-prev"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={page <= 1}
                    onClick={() => onPageChange(page - 1)}
                >
                    上一页
                </Button>
                <Button
                    id="my-transfers-next"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={page * PAGE_SIZE >= total}
                    onClick={() => onPageChange(page + 1)}
                >
                    下一页
                </Button>
            </div>
        </div>
    )
}

export function MyTransfersPage({ portal = false }: { portal?: boolean }) {
    const [page, setPage] = useState(1)
    const [previewId, setPreviewId] = useState<string | null>(null)
    const openPreview = useCallback((job: BackgroundJobView) => {
        // The controlled sheet returns focus to the element focused before opening.
        restoreListRowFocus(job.id)
        setPreviewId(job.id)
    }, [])
    const columns = useMemo(() => transferColumns(openPreview), [openPreview])
    const query = useQuery({
        queryKey: [
            portal ? "supplier-portal" : "account",
            "data-transfers",
            page,
        ],
        queryFn: () =>
            apiGet<{ items: BackgroundJobView[]; total: number }>(
                portal
                    ? "/supplier-portal/data-transfers"
                    : "/account/data-transfers",
                {
                    page,
                    page_size: PAGE_SIZE,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
                portal ? { session: "supplier-portal" } : undefined,
            ),
        refetchInterval: 5000,
    })
    const total = query.data?.total ?? 0

    return (
        <>
            <ListWorkspacePage
                title={TITLE}
                actions={[
                    {
                        id: "my-transfers-refresh",
                        actionKey: "refresh",
                        label: "刷新",
                        icon: RefreshCwIcon,
                        tier: "quiet",
                        onClick: () => void query.refetch(),
                    },
                ]}
                notices={
                    <p className="text-xs text-muted-foreground">
                        关闭页面后任务会继续在后台处理。导出和结果文件保留 7
                        天；如导入内容有确认期限，请在有效期内完成确认，下载时将校验当前账号权限。
                    </p>
                }
                surface={{
                    ariaLabel: TITLE,
                    className: "@container/transfers",
                    tableClassName:
                        "flex flex-col [&_[data-column-id=actions]]:min-w-40 @max-2xl/transfers:[&_[data-column-pinned]]:inset-x-auto! @max-2xl/transfers:[&_[data-pinned-edge]]:shadow-none!",
                    table: (
                        <>
                            <ListDataTable
                                id="my-transfers-table"
                                caption={TITLE}
                                data={query.data?.items ?? EMPTY_ROWS}
                                columns={columns}
                                defaultColumnPinning={{
                                    left: ["job"],
                                    right: ["actions"],
                                }}
                                getRowId={(job) => job.id}
                                rowLabel={transferLabel}
                                rowCount={total}
                                showToolbarSummary={false}
                                showPagination={false}
                                loading={query.isFetching}
                                isError={query.isError}
                                error={query.error}
                                onRetry={() => void query.refetch()}
                                emptyState={
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="暂无文件任务"
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                    />
                                }
                                onRowPreview={openPreview}
                                highlightedRowId={previewId ?? undefined}
                            />
                            <TransferPagination
                                page={page}
                                total={total}
                                onPageChange={setPage}
                            />
                        </>
                    ),
                }}
            />
            <MyTransferPreview
                jobId={previewId}
                portal={portal}
                onClose={() => setPreviewId(null)}
            />
        </>
    )
}
