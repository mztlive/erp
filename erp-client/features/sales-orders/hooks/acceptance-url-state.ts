"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

export function useAcceptanceWorkspaceUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const workItemId = searchParams.get("workItemId")

    /** 交付记录筛选随 URL 持久化，刷新/分享不丢失（契约：参数与控件一一对应）。 */
    const [remainingOnly, setRemainingOnlyState] = React.useState(
        searchParams.get("remainingOnly") !== "false",
    )

    const setRemainingOnly = React.useCallback(
        (next: boolean) => {
            setRemainingOnlyState(next)
            const params = new URLSearchParams(searchParams.toString())
            params.set("section", "acceptance")
            params.set("remainingOnly", next ? "1" : "0")
            router.replace(`${pathname}?${params.toString()}`, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    return { workItemId, remainingOnly, setRemainingOnly }
}
