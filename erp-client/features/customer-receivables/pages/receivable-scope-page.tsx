"use client"

import * as React from "react"

import { PageScaffold } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { downloadListCsv } from "@/lib/list-export"
import { getErrorMessage } from "@/lib/api/errors"
import { toast } from "@/components/ui/toast"
import { exportReceivableScopeCsv } from "@/features/customer-receivables/lib/scoped-export"
import { useReceivableScopeDetailQuery } from "@/features/customer-receivables/hooks/scoped-queries"
import { useReceivableScopeListQuery } from "@/features/customer-receivables/hooks/scoped-queries"
import { ReceivableScopeDetailPreview } from "@/features/customer-receivables/components/scoped-detail-preview"
import { ReceivableScopeListPane } from "@/features/customer-receivables/pages/components/receivable-scope-list-pane"
import { useReceivableScopeUrlState } from "@/features/customer-receivables/pages/hooks/use-receivable-scope-url-state"

/** 客户往来范围页：URL 与 Query key 同步范围条件，跨页绑定范围版本。 */
export function ReceivableScopePage() {
    const urlState = useReceivableScopeUrlState()
    const listQuery = useReceivableScopeListQuery(urlState.query)
    const [preview, setPreview] = React.useState<{
        kind: "receivable" | "receipt" | "invoice"
        id: string
    } | null>(null)
    const detailQuery = useReceivableScopeDetailQuery(
        preview?.kind ?? null,
        preview?.id ?? null,
    )
    const [exporting, setExporting] = React.useState(false)

    async function handleExport() {
        if (exporting) return
        setExporting(true)
        try {
            const result = await exportReceivableScopeCsv(urlState.query)
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

    return (
        <PageScaffold density="compact" className="space-y-4">
            <ListWorkspaceHeader
                eyebrow="财务"
                title="客户往来（按数据范围）"
                description="按负责销售与登记/核销经办人查询；部分受限仅显示获授权份额。"
            />
            <ReceivableScopeListPane
                urlState={urlState}
                data={listQuery.data}
                isPending={listQuery.isPending}
                isError={listQuery.isError}
                error={listQuery.error}
                onRetry={() => void listQuery.refetch()}
                onExport={() => void handleExport()}
                exporting={exporting}
                onPreview={(kind, id) => setPreview({ kind, id })}
                previewId={preview?.id}
            />
            <ReceivableScopeDetailPreview
                open={preview != null}
                data={detailQuery.data}
                scopeSummary={listQuery.data?.scopeSummary}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                onClose={() => setPreview(null)}
            />
        </PageScaffold>
    )
}
