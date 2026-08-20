"use client"

import { QuickPreviewSheet } from "@/components/business"
import { AuditEventBody } from "@/features/access-audit/components/audit-event-body"
import { EffectiveAccessBody } from "@/features/access-audit/components/effective-access-body"
import {
    useAuditEventQuery,
    useEffectiveAccessQuery,
} from "@/features/access-audit/hooks/queries"

type AccessPreviewSheetsProps = {
    explainSubject: { type: "ROLE" | "USER"; id: string } | null
    eventOpenId: string | null
    effectiveQuery: ReturnType<typeof useEffectiveAccessQuery>
    eventQuery: ReturnType<typeof useAuditEventQuery>
    closeExplain: () => void
    closeEvent: () => void
    restoreRowFocus: () => void
}

function AccessPreviewSheets({
    explainSubject,
    eventOpenId,
    effectiveQuery,
    eventQuery,
    closeExplain,
    closeEvent,
    restoreRowFocus,
}: AccessPreviewSheetsProps) {
    return (
        <>
            {/* 有效权限解释 Sheet — 服务端投影，前端不合并 */}
            <QuickPreviewSheet
                open={Boolean(explainSubject)}
                onOpenChange={(open) => {
                    if (!open) closeExplain()
                }}
                size="detail"
                onOpenChangeComplete={(open) => {
                    if (!open) restoreRowFocus()
                }}
                title="有效权限解释"
                description="此处展示的权限结果为系统统一计算，可能与页面其它位置显示略有差异。"
            >
                <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
                    <EffectiveAccessBody query={effectiveQuery} />
                </div>
            </QuickPreviewSheet>

            {/* 审计详情 — 敏感字段仅字段名 + 已变更 */}
            <QuickPreviewSheet
                open={Boolean(eventOpenId)}
                onOpenChange={(open) => {
                    if (!open) closeEvent()
                }}
                size="detail"
                onOpenChangeComplete={(open) => {
                    if (!open) restoreRowFocus()
                }}
                title="审计事件详情"
                description="追加式事件只读；不展示敏感旧值/新值或密钥。"
            >
                <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
                    <AuditEventBody query={eventQuery} />
                </div>
            </QuickPreviewSheet>
        </>
    )
}

export { AccessPreviewSheets }
