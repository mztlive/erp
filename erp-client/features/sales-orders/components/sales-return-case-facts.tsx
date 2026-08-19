"use client"

import type { ReactNode } from "react"

import type { SalesReturnCaseRow } from "@/features/sales-orders/api/sales-return-cases"
import { cn } from "@/lib/utils"

/**
 * 销售退货处理单事实区。
 *
 * SalesReturnCase 为 NO_APPROVAL：只展示处理号、类型、路线和履约分工状态，
 * 不嵌入绑定卡、运行摘要或决定弹窗。待仓储验收 / 待采购处理 / 待财务处理
 * 不得渲染为审批复核。
 */
export function SalesReturnCaseFacts({ row }: { row: SalesReturnCaseRow }) {
    return (
        <div className="space-y-4">
            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 xl:grid-cols-3">
                <FactField label="退货处理号" value={row.returnNo} numeric />
                <FactField label="处理类型" value={row.caseTypeLabel} />
                <FactField label="退货路线" value={row.returnRouteLabel} />
                <FactField label="当前状态" value={row.statusLabel} />
                <FactField
                    label="明细行数"
                    value={`${row.lines.length} 行`}
                    numeric
                />
                <FactField label="原因" value={row.reason || "—"} />
            </dl>
        </div>
    )
}

/**
 * 销售退货只读字段。仅展示履约事实，不提供审批动作。
 *
 * @param label 字段中文名。
 * @param value 已映射的业务文案。
 * @param numeric 是否按编号样式渲染。
 */
function FactField({
    label,
    value,
    numeric,
}: {
    label: string
    value: ReactNode
    numeric?: boolean
}) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className={cn("mt-0.5 truncate text-sm", numeric && "num")}>
                {value}
            </dd>
        </div>
    )
}
