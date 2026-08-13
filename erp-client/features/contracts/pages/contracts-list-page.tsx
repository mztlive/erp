"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { DownloadIcon, FileUpIcon, PrinterIcon, SearchIcon } from "lucide-react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FilterChip,
    FormalActionResult,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    PageActions,
    PageHeader,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractPreviewPanel } from "@/features/contracts/components/contract-preview-panel"
import { ContractUploadDialog } from "@/features/contracts/components/contract-upload-dialog"
import {
    computeContractMetrics,
    contractMetricLabel,
    filterContracts,
    type ContractMetricFilter,
} from "@/features/contracts/lib/filter-contracts"
import { sortRows } from "@/features/contracts/lib/contract-list-sort"
import {
    contractsUrlCodec,
    type ContractsUrlState,
} from "@/features/contracts/lib/contracts-url-state"
import {
    useContractCenterQuery,
    useContractsQuery,
    useCreateContractExportJobMutation,
} from "@/features/contracts/hooks/queries"
import { useContractListColumns } from "@/features/contracts/hooks/use-contract-list-columns"
import type {
    ContractExportJob,
    UploadContractPdfResult,
} from "@/features/contracts/types"

export function ContractsListPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const contractsQuery = useContractsQuery()
    const allRows = React.useMemo(
        () => contractsQuery.data ?? [],
        [contractsQuery.data],
    )

    const url = React.useMemo(
        () => contractsUrlCodec.parse(searchParams),
        [searchParams],
    )
    const { q, metric, page, pageSize, sort, dir, customerId } = url

    const [searchDraft, setSearchDraft] = React.useState(q ?? "")
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [paperId, setPaperId] = React.useState<string | null>(null)
    const [uploadOpen, setUploadOpen] = React.useState(Boolean(customerId))
    const [exportJob, setExportJob] = React.useState<ContractExportJob | null>(
        null,
    )
    const [actionResult, setActionResult] = React.useState<{
        status: "succeeded" | "blocked"
        title: string
        description: string
        facts?: Array<{ label: string; value: string }>
        nextHref?: string
    } | null>(null)

    const exportMutation = useCreateContractExportJobMutation()

    /** URL-first：筛选/分页/排序全部写 URL，浏览器后退与刷新一致。 */
    const pushUrl = React.useCallback(
        (patch: Partial<ContractsUrlState>) => {
            const next = { ...url, ...patch }
            router.replace(`${pathname}${contractsUrlCodec.build(next)}`, {
                scroll: false,
            })
        },
        [pathname, router, url],
    )

    React.useEffect(() => {
        setSearchDraft(q ?? "")
    }, [q])

    // 防抖即时搜索（300ms）+ Enter 兜底；写 URL 并回第 1 页。
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === (q ?? "")) return
            pushUrl({ q: searchDraft.trim() || undefined, page: 1 })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- pushUrl 以当前 URL 快照为准
    }, [searchDraft])

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                return
            }
            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="contracts-search"]',
                    )
                    ?.focus()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [])

    const filtered = React.useMemo(() => {
        let rows = filterContracts(allRows, {
            search: q ?? "",
            metricKey: metric,
            statusFilter: "all",
        })
        if (customerId) {
            rows = rows.filter((r) => r.customer.customerId === customerId)
        }
        return rows
    }, [allRows, customerId, metric, q])

    const sorting = React.useMemo<SortingState>(
        () => (sort ? [{ id: sort, desc: dir === "desc" }] : []),
        [dir, sort],
    )

    const sorted = React.useMemo(
        () => sortRows(filtered, sorting),
        [filtered, sorting],
    )

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, page - 1), pageSize }),
        [page, pageSize],
    )

    const pageRows = React.useMemo(() => {
        const start = pagination.pageIndex * pagination.pageSize
        return sorted.slice(start, start + pagination.pageSize)
    }, [pagination.pageIndex, pagination.pageSize, sorted])

    const metrics = React.useMemo(
        () => computeContractMetrics(allRows),
        [allRows],
    )

    /** 客户锁定来自 URL customerId：界面给出可移除 chip 与清除入口。 */
    const lockedCustomer = React.useMemo(() => {
        if (!customerId) return null
        return (
            allRows.find((r) => r.customer.customerId === customerId)
                ?.customer ?? null
        )
    }, [allRows, customerId])

    const previewRow = React.useMemo(
        () => allRows.find((item) => item.contractId === previewId) ?? null,
        [allRows, previewId],
    )

    const previewDetailQuery = useContractCenterQuery(previewId ?? "")
    const paperDetailQuery = useContractCenterQuery(paperId ?? "")

    const handleUploadSuccess = React.useCallback(
        (result: UploadContractPdfResult) => {
            setActionResult({
                status: "succeeded",
                title: "合同 PDF 已归档",
                description:
                    "已形成可追溯的合同版本，可直接选择用于新建销售单。",
                facts: [
                    { label: "合同号", value: result.contractNo },
                    { label: "修订", value: `v${result.revisionNo}` },
                    { label: "文件", value: result.fileName },
                    {
                        label: "上传时间",
                        value: result.uploadedAt.slice(0, 19).replace("T", " "),
                    },
                    { label: "下一步", value: "查看详情核对或新建销售单" },
                ],
                nextHref: `/sales/contracts/${result.contractId}`,
            })
        },
        [],
    )

    const filterSnapshotLabel = React.useMemo(() => {
        const parts = [
            `指标=${contractMetricLabel(metric)}`,
            (q ?? "").trim() ? `搜索=${(q ?? "").trim()}` : "搜索=空",
            lockedCustomer ? `客户=${lockedCustomer.displayName}` : null,
        ].filter(Boolean)
        return parts.join(" · ")
    }, [lockedCustomer, metric, q])

    const handleExport = React.useCallback(async () => {
        if (filtered.length === 0) return
        const job = await exportMutation.mutateAsync({
            rowCount: filtered.length,
            filterSnapshotLabel,
        })
        setExportJob(job)
        setActionResult({
            status: "succeeded",
            title: "导出完成",
            description:
                "已生成 CSV 文件，内容按当前筛选生成；下载时将重新校验权限。",
            facts: [
                { label: "筛选结果", value: job.filterSnapshotLabel },
                { label: "行数", value: String(job.rowCount) },
                { label: "文件", value: job.downloadLabel },
            ],
        })
    }, [exportMutation, filterSnapshotLabel, filtered.length])

    const columns = useContractListColumns({
        onPreview: setPreviewId,
        onPaper: setPaperId,
    })

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const handleSearchCommit = React.useCallback(
        (value: string) => {
            setSearchDraft(value)
            pushUrl({ q: value.trim() || undefined, page: 1 })
        },
        [pushUrl],
    )

    const handleMetricChange = React.useCallback(
        (next: ContractMetricFilter) => {
            pushUrl({ metric: next, page: 1 })
        },
        [pushUrl],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort: head?.id,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    const handleClearCustomerLock = React.useCallback(() => {
        pushUrl({ customerId: undefined })
    }, [pushUrl])

    /** P4：清 q + 全部筛选参数（含 customerId 锁定）+ 分页回 1；保留排序。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        pushUrl({
            q: undefined,
            metric: "all",
            customerId: undefined,
            page: 1,
        })
    }, [pushUrl])

    const isFiltered =
        (q ?? "").trim() !== "" || metric !== "all" || Boolean(customerId)

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="合同"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "contracts", label: "合同", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            contractsQuery.isError
                                ? "查询失败"
                                : contractsQuery.data
                                  ? new Date(
                                        contractsQuery.dataUpdatedAt,
                                    ).toLocaleTimeString("zh-CN", {
                                        hour: "2-digit",
                                        minute: "2-digit",
                                    })
                                  : "正在查询"
                        }
                        dateTime={
                            contractsQuery.data
                                ? new Date(
                                      contractsQuery.dataUpdatedAt,
                                  ).toISOString()
                                : undefined
                        }
                        state={
                            contractsQuery.isError
                                ? "failed"
                                : contractsQuery.isFetching
                                  ? "syncing"
                                  : "fresh"
                        }
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                disabled:
                                    filtered.length === 0 ||
                                    exportMutation.isPending,
                                onClick: () => {
                                    void handleExport()
                                },
                            },
                            {
                                actionKey: "upload",
                                label: "上传合同 PDF",
                                icon: FileUpIcon,
                                onClick: () => setUploadOpen(true),
                            },
                        ]}
                    />
                }
            />

            {actionResult ? (
                <FormalActionResult
                    status={actionResult.status}
                    title={actionResult.title}
                    description={actionResult.description}
                    facts={actionResult.facts}
                    actions={
                        actionResult.nextHref ? (
                            <Button
                                type="button"
                                size="sm"
                                render={<Link href={actionResult.nextHref} />}
                            >
                                查看详情
                            </Button>
                        ) : null
                    }
                />
            ) : null}

            {exportJob ? (
                <FormalActionResult
                    status="succeeded"
                    title="合同导出完成"
                    description={`共 ${exportJob.rowCount} 条，内容按当前筛选生成；下载时将重新校验权限。`}
                    facts={[
                        { label: "文件", value: exportJob.downloadLabel },
                        { label: "行数", value: String(exportJob.rowCount) },
                    ]}
                />
            ) : null}

            <MetricStrip columns={5} aria-label="合同快速筛选">
                <MetricFilterItem
                    label="全部合同"
                    value={metrics.all}
                    detail="当前业务范围"
                    active={metric === "all"}
                    onClick={() => handleMetricChange("all")}
                />
                <MetricFilterItem
                    label="有效"
                    value={metrics.effective}
                    detail="可关联建单"
                    active={metric === "effective"}
                    onClick={() => handleMetricChange("effective")}
                />
                <MetricFilterItem
                    label="30 天内到期"
                    value={metrics.expiring_30d}
                    detail="将到期提醒"
                    active={metric === "expiring_30d"}
                    onClick={() => handleMetricChange("expiring_30d")}
                />
                <MetricFilterItem
                    label="已到期"
                    value={metrics.expired}
                    detail="历史可追溯"
                    active={metric === "expired"}
                    onClick={() => handleMetricChange("expired")}
                />
                <MetricFilterItem
                    label="已终止"
                    value={metrics.terminated}
                    detail="不再履行"
                    active={metric === "terminated"}
                    onClick={() => handleMetricChange("terminated")}
                />
            </MetricStrip>

            <BusinessTableFrame
                title="合同列表"
                description={
                    metric === "all" && !(q ?? "").trim()
                        ? "按将到期优先排序展示当前业务范围内的合同。"
                        : `当前筛选：${contractMetricLabel(metric)}${
                              (q ?? "").trim() ? ` · “${(q ?? "").trim()}”` : ""
                          }`
                }
                toolbar={
                    <ListToolbar
                        search={
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    data-slot="contracts-search"
                                    value={searchDraft}
                                    onChange={(event) => {
                                        setSearchDraft(event.target.value)
                                    }}
                                    onKeyDown={(event) => {
                                        if (event.key === "Enter") {
                                            handleSearchCommit(searchDraft)
                                        }
                                    }}
                                    placeholder="合同号、客户、结算主体、负责人"
                                    aria-label="搜索合同"
                                />
                            </InputGroup>
                        }
                        secondary={
                            lockedCustomer ? (
                                <FilterChip
                                    label={`客户：${lockedCustomer.displayName}`}
                                    onClear={handleClearCustomerLock}
                                    clearLabel="清除客户锁定"
                                />
                            ) : undefined
                        }
                        actions={
                            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                <span aria-live="polite">
                                    共 {sorted.length.toLocaleString("zh-CN")}{" "}
                                    条
                                </span>
                                {isFiltered ? (
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        onClick={clearAllFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : null}
                            </div>
                        }
                    />
                }
                table={
                    contractsQuery.isError ? (
                        <BusinessFailureState
                            title="合同列表加载失败"
                            error={contractsQuery.error}
                            onRetry={() => {
                                void contractsQuery.refetch()
                            }}
                        />
                    ) : pageRows.length === 0 && !contractsQuery.isPending ? (
                        <BusinessEmptyState
                            kind={isFiltered ? "filter" : "no-data"}
                            title={isFiltered ? undefined : "还没有合同"}
                            description={
                                isFiltered
                                    ? "换一个关键词或清除筛选后再试。"
                                    : "上传第一份合同 PDF，即可用于新建销售单。"
                            }
                            action={
                                isFiltered ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={clearAllFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={() => setUploadOpen(true)}
                                    >
                                        <FileUpIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        上传合同 PDF
                                    </Button>
                                )
                            }
                        />
                    ) : (
                        <DataTable
                            data={pageRows}
                            columns={columns}
                            getRowId={(row) => row.contractId}
                            rowCount={sorted.length}
                            sorting={sorting}
                            onSortingChange={handleSortingChange}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            loading={contractsQuery.isPending}
                            layout="flush"
                            density="compact"
                            defaultColumnPinning={{
                                left: ["contractNo"],
                                right: ["actions"],
                            }}
                            onRowPreview={(row) => setPreviewId(row.contractId)}
                            onRowOpen={(row) => setPreviewId(row.contractId)}
                        />
                    )
                }
            />

            <QuickPreviewSheet
                open={previewRow != null}
                onOpenChange={(open) => {
                    if (!open) setPreviewId(null)
                }}
                size="detail"
                title={previewRow?.customer.displayName ?? "合同预览"}
                identity={
                    previewRow ? (
                        <span className="num">
                            {previewRow.contractNo} · v{previewRow.revisionNo}
                        </span>
                    ) : null
                }
                summary={
                    previewRow ? (
                        <div className="flex flex-wrap items-center gap-2">
                            <BusinessStatusBadge
                                context="preview"
                                label={previewRow.statusLabel}
                                tone={previewRow.statusTone}
                            />
                            {previewRow.expiringWithin30Days ? (
                                <Badge variant="warning">将到期</Badge>
                            ) : null}
                            <span className="text-xs text-muted-foreground">
                                关联销售 {previewRow.salesOrderCount} 张
                            </span>
                        </div>
                    ) : null
                }
                footer={
                    previewRow ? (
                        <>
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => setPreviewId(null)}
                            >
                                关闭
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                disabled={
                                    !previewRow.allowedActions.includes("PRINT")
                                }
                                title={
                                    previewRow.actionBlockers.find(
                                        (b) => b.action === "PRINT",
                                    )?.message
                                }
                                onClick={() =>
                                    setPaperId(previewRow.contractId)
                                }
                            >
                                <PrinterIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                纸质预览
                            </Button>
                            <Button
                                type="button"
                                render={
                                    <Link
                                        href={`/sales/contracts/${previewRow.contractId}`}
                                    />
                                }
                            >
                                查看详情
                            </Button>
                        </>
                    ) : null
                }
            >
                {previewRow ? (
                    <ContractPreviewPanel
                        row={previewRow}
                        detail={previewDetailQuery.data}
                        detailLoading={previewDetailQuery.isPending}
                    />
                ) : null}
            </QuickPreviewSheet>

            <ContractPaperDialog
                contract={paperDetailQuery.data ?? null}
                open={paperId != null && paperDetailQuery.data != null}
                onOpenChange={(open) => {
                    if (!open) setPaperId(null)
                }}
            />

            <ContractUploadDialog
                open={uploadOpen}
                onOpenChange={setUploadOpen}
                initialCustomerId={customerId ?? ""}
                onSuccess={handleUploadSuccess}
            />
        </PageScaffold>
    )
}
