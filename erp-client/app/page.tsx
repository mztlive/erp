"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { isAuthenticated } from "@/lib/api/session"

/**
 * 根路由：按登录态分流（未登录 → /login，已登录 → /workspace 我的工作台）。
 */
export default function HomePage() {
    const router = useRouter()

    React.useEffect(() => {
        if (isAuthenticated()) {
            router.replace("/workspace")
        } else {
            router.replace("/login")
        }
    }, [router])

    return (
        <div className="flex min-h-svh items-center justify-center bg-background text-sm text-muted-foreground">
            正在进入系统…
        </div>
    )
}
