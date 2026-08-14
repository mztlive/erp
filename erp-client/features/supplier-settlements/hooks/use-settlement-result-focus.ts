"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"

/** 动作结果确定（成功/结果待确认）后把焦点移到结果面板。 */
function useSettlementResultFocus(
    result: ResultState,
    resultRef: React.RefObject<HTMLDivElement | null>,
) {
    React.useEffect(() => {
        if (result?.status === "succeeded" || result?.status === "unknown") {
            resultRef.current?.focus()
        }
    }, [result, resultRef])
}

export { useSettlementResultFocus }
