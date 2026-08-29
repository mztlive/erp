"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

export function useAcceptanceWorkspaceUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const workItemId = searchParams.get("workItemId")
    const isRegister = searchParams.get("mode") === "register"

    const setRegisterMode = React.useCallback(
        (next: boolean, options?: { clearTask?: boolean }) => {
            const params = new URLSearchParams(searchParams.toString())
            params.set("section", "acceptance")
            if (next) params.set("mode", "register")
            else params.delete("mode")
            if (options?.clearTask) {
                params.delete("workItemId")
                params.delete("queueContextId")
            }
            const qs = params.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    return { workItemId, isRegister, setRegisterMode }
}
