"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { parseSection } from "@/features/mall-consumption-orders/lib/url-state"
import type { ObjectCenterSectionId } from "@/features/mall-consumption-orders/types"

/**
 * 对象中心 URL 状态：section / fact 双参数与列表返回地址。
 * section 非法值回退 overview；fact 只在 facts 分区携带。
 */
export function useObjectCenterSection() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const section = parseSection(searchParams.get("section"))
    const factId = searchParams.get("fact") ?? undefined
    /** 从列表页进入时携带的返回地址（保留筛选/分页上下文） */
    const backToListHref =
        searchParams.get("returnTo") ?? "/commerce/consumption-orders"

    const setSection = React.useCallback(
        (next: ObjectCenterSectionId, fact?: string) => {
            const sp = new URLSearchParams(searchParams.toString())
            sp.set("section", next)
            if (fact) sp.set("fact", fact)
            else if (next !== "facts") sp.delete("fact")
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    return { section, factId, backToListHref, setSection }
}
