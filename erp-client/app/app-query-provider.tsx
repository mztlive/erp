"use client"

import type { ReactNode } from "react"
import { QueryProvider } from "@/components/providers/query-provider"
import { subscribeScopeCache } from "@/features/data-scope/cache"

/** 应用根层 QueryProvider 装配：将 data-scope 缓存订阅注入纯 Provider 底座。 */
export function AppQueryProvider({ children }: { children: ReactNode }) {
    return (
        <QueryProvider subscribeCache={subscribeScopeCache}>
            {children}
        </QueryProvider>
    )
}
