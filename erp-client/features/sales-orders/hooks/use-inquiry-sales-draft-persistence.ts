"use client"
import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { writeInquiryLocalValue } from "@/lib/inquiry-local"
import { toast } from "@/components/ui/toast"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import { inquirySaleDraftKey } from "../lib/inquiry-sales-storage"

export function useInquirySalesDraftPersistence(
    form: SalesOrderCreateFormApi,
    userId: string | undefined,
    scope: string,
) {
    const values = useSelector(form.store, (state) => state.values)
    const reported = React.useRef(false)
    React.useEffect(() => {
        if (!userId) return
        try {
            writeInquiryLocalValue(inquirySaleDraftKey(userId, scope), values)
        } catch {
            if (!reported.current) {
                reported.current = true
                toast.add({
                    title: "本地暂存失败",
                    description:
                        "当前输入仍保留在页面，请保持页面打开并稍后重试。",
                    type: "error",
                })
            }
        }
    }, [userId, scope, values])
}
