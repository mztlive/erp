"use client"

import * as React from "react"

import { SystemFeedback } from "@/components/business/system-feedback"
import { toast } from "@/components/ui/toast"
import { formatTime } from "@/lib/datetime"

export type SalesOrderCreateAlertsProps = {
    /** 提交命令失败或结果待确认时的提示。 */
    formalFailure: {
        error: unknown
        unknown: boolean
        description: string
    } | null
    /** 新建销售单 mutation 错误；为 null 表示未出错。 */
    createError: Error | null
    draftSaved: { documentNumber: string; savedAt: Date } | null
}

export function SalesOrderCreateAlerts({
    formalFailure,
    createError,
    draftSaved,
}: SalesOrderCreateAlertsProps) {
    const announcedDraftRef = React.useRef<string | null>(null)

    React.useEffect(() => {
        if (!draftSaved) return

        const announcementKey = `${draftSaved.documentNumber}:${draftSaved.savedAt.toISOString()}`
        if (announcedDraftRef.current === announcementKey) return
        announcedDraftRef.current = announcementKey

        toast.add({
            title: "草稿已保存",
            description: `销售单 ${draftSaved.documentNumber} 已于 ${formatTime(draftSaved.savedAt.toISOString())} 保存为草稿；当前内容仍保留在本页。`,
            type: "success",
            timeout: 4000,
        })
    }, [draftSaved])

    return (
        <SystemFeedback
            problemId="sales-order-create"
            operation
            outcomeUnknown={formalFailure?.unknown}
            error={formalFailure?.error ?? createError}
            title={
                formalFailure?.unknown ? "处理结果待确认" : "销售单操作未完成"
            }
            description={formalFailure?.description}
        />
    )
}
