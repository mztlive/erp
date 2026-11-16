"use client"

import * as React from "react"

import { PageScaffold } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { useScopeListExport } from "@/lib/use-scope-list-export"
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
    const { exporting, handleExport } = useScopeListExport(() =>
        exportReceivableScopeCsv(urlState.query),
    )

    return (
        <PageScaffold density="compact" className="space-y-4">
            <ListWorkspaceHeader
                eyebrow="财务"
                title="客户往来汇总"
                description="按负责销售与登记/核销经办人查询；部分金额受限时仅显示本人权限内金额。"
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
