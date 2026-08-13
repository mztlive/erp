"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

export function ListRouteRedirect({
    href,
    label,
}: {
    href: string
    label: string
}) {
    const router = useRouter()
    React.useEffect(() => {
        router.replace(href)
    }, [href, router])
    return (
        <div className="flex min-h-[12rem] items-center justify-center text-sm text-muted-foreground">
            正在打开{label}…
        </div>
    )
}
