"use client"

import { useMemo } from "react"
import { useRouter as useNextRouter } from "next/navigation"
import { isWorkspaceFrame, navigateWorkspace } from "@/lib/workspace-tabs"

export { usePathname, useSearchParams, useParams } from "next/navigation"

/** Public Next router adapter: documents open in retained tabs; filters remain local. */
export function useRouter(): ReturnType<typeof useNextRouter> {
    const router = useNextRouter()
    return useMemo(
        () => ({
            ...router,
            push: (href, options) => {
                if (!navigateWorkspace(href)) {
                    if (isWorkspaceFrame()) router.replace(href, options)
                    else router.push(href, options)
                }
            },
            replace: (href, options) => {
                if (!navigateWorkspace(href, true))
                    router.replace(href, options)
            },
            back: () => {
                if (isWorkspaceFrame()) window.parent.history.back()
                else router.back()
            },
            forward: () => {
                if (isWorkspaceFrame()) window.parent.history.forward()
                else router.forward()
            },
        }),
        [router],
    )
}
