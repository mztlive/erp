"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, DownloadIcon } from "lucide-react"

import {
    ListWorkspacePage,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { ReceivableScopeDetail } from "@/features/customer-receivables/api/scoped"
import { ReceivableScopeDetailPreview } from "@/features/customer-receivables/components/scoped-detail-preview"
import {
    useReceivableScopeDetailQuery,
    useReceivableScopeListQuery,
} from "@/features/customer-receivables/hooks/scoped-queries"
import { exportReceivableScopeCsv } from "@/features/customer-receivables/lib/scoped-export"
import { ReceivableScopeListPane } from "@/features/customer-receivables/pages/components/receivable-scope-list-pane"
import { useReceivableScopeUrlState } from "@/features/customer-receivables/pages/hooks/use-receivable-scope-url-state"
import { VIEW_LABEL } from "@/features/customer-receivables/types"
import { useScopeListExport } from "@/lib/use-scope-list-export"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

const TITLE = "客户往来汇总"

/** 客户往来范围页：URL 与 Query key 同步范围条件，跨页绑定范围版本。 */
export function ReceivableScopePage() {
    const urlState = useReceivableScopeUrlState()
    const { account, canReadView, canPreview, visibleViews, view } = urlState
    const canReadList = account.isSuccess && canReadView
    const listQuery = useReceivableScopeListQuery(urlState.query, canReadList)
    const [preview, setPreview] = React.useState<{
        kind: ReceivableScopeDetail["kind"]
        id: string
    } | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    useSearchHotkey(urlState.searchInputRef)
    const previewOpen =
        canReadList &&
        canPreview &&
        preview?.kind === (view === "sales_invoice" ? "invoice" : view)
    React.useEffect(() => {
        if (!previewOpen) setPreview(null)
    }, [previewOpen])
    const detailQuery = useReceivableScopeDetailQuery(
        previewOpen ? preview.kind : null,
        previewOpen ? preview.id : null,
    )
    const { exporting, handleExport } = useScopeListExport(() =>
        exportReceivableScopeCsv(urlState.query),
    )
    // 跨视图保留的查询占位数据不能当成当前视图的范围与统计。
    const data =
        canReadList && listQuery.data?.view === view
            ? listQuery.data
            : undefined
    const showViews =
        account.isSuccess &&
        (visibleViews.length > 1 || (!canReadView && visibleViews.length > 0))
    const exportDisabledReason = listQuery.isError
        ? "请在查询成功后导出"
        : listQuery.isPlaceholderData
          ? "请等待当前查询完成"
          : !data?.total
            ? "有查询结果后可导出"
            : undefined

    return (
        <ListWorkspacePage
            title={TITLE}
            loading={account.isPending}
            views={
                showViews ? (
                    <Tabs
                        value={canReadView ? view : null}
                        onValueChange={(next) => {
                            const nextView = visibleViews.find(
                                (item) => item === next,
                            )
                            if (!nextView || nextView === view) return
                            setPreview(null)
                            urlState.changeView(nextView)
                        }}
                    >
                        <TabsList
                            variant="line"
                            aria-label="客户往来类型"
                            className="w-full gap-5 overflow-x-auto"
                        >
                            {visibleViews.map((item) => (
                                <TabsTrigger
                                    key={item}
                                    value={item}
                                    id={`customer-receivables-scope-view-${item}`}
                                    className="min-h-workspace-view font-normal data-active:font-medium md:min-h-12"
                                >
                                    {VIEW_LABEL[item]}
                                </TabsTrigger>
                            ))}
                        </TabsList>
                    </Tabs>
                ) : undefined
            }
            headerActions={
                <div className="flex items-center gap-1">
                    <Button
                        id="customer-receivables-scope-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        render={
                            <Link
                                href={`/finance/customer-accounts?view=${view}`}
                            />
                        }
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        返回台账
                    </Button>
                    {canReadList ? (
                        <LoadingButton
                            id="customer-receivables-scope-export"
                            type="button"
                            loading={exporting}
                            variant="ghost"
                            size="sm"
                            className={listWorkspaceStyles.quietButton}
                            disabled={exportDisabledReason !== undefined}
                            title={exportDisabledReason}
                            onClick={() => void handleExport()}
                        >
                            <DownloadIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {exporting ? "导出中…" : "导出当前范围"}
                        </LoadingButton>
                    ) : null}
                </div>
            }
        >
            <ReceivableScopeListPane
                urlState={urlState}
                data={data}
                loading={
                    canReadList && (listQuery.isPending || listQuery.isFetching)
                }
                isError={account.isError || (canReadList && listQuery.isError)}
                error={account.isError ? account.error : listQuery.error}
                placeholder={listQuery.isPlaceholderData}
                onRetry={() => {
                    if (account.isError) void account.refetch()
                    else void listQuery.refetch()
                }}
                onPreview={
                    canPreview && canReadList
                        ? (kind, id) => {
                              lastFocusedRowId.current = id
                              setPreview({ kind, id })
                          }
                        : undefined
                }
                previewId={previewOpen ? preview.id : undefined}
            />
            <ReceivableScopeDetailPreview
                open={previewOpen}
                data={detailQuery.data}
                scopeSummary={data?.scopeSummary}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                onClose={() => setPreview(null)}
                finalFocus={() => {
                    const rowId = lastFocusedRowId.current
                    const row = rowId
                        ? document.querySelector<HTMLElement>(
                              `[data-slot="list-work-surface"] [data-row-id="${CSS.escape(rowId)}"][tabindex]`,
                          )
                        : null
                    return row ?? urlState.searchInputRef.current ?? true
                }}
            />
        </ListWorkspacePage>
    )
}
