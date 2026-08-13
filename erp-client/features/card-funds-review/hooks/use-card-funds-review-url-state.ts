"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ReadonlyURLSearchParams } from "next/navigation"

import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueView,
} from "@/features/card-funds-review/types"

export function useCardFundsReviewUrlState(): {
    scope: "mine" | "role_pool"
    type: "all" | "opening" | "delta"
    status: "pending" | "held"
    due: "all" | "today" | "overdue"
    q: string | undefined
    currentWorkItemId: string | undefined
    queueContextId: string
    autoNext: boolean
    searchInput: string
    setSearchInput: React.Dispatch<React.SetStateAction<string>>
    setAutoNext: (on: boolean) => void
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    pathname: string
    searchParams: ReadonlyURLSearchParams
    router: ReturnType<typeof useRouter>
} {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const scope: "mine" | "role_pool" =
        searchParams.get("scope") === "role_pool" ? "role_pool" : "mine"
    const typeParam = searchParams.get("type")
    const type: "all" | "opening" | "delta" =
        typeParam === "opening" || typeParam === "delta" ? typeParam : "all"
    const statusParam = searchParams.get("status")
    const status: "pending" | "held" =
        statusParam === "held" ? "held" : "pending"
    const dueParam = searchParams.get("due")
    const due: "all" | "today" | "overdue" =
        dueParam === "today" || dueParam === "overdue" ? dueParam : "all"
    const q = searchParams.get("q") ?? undefined
    const currentWorkItemId = searchParams.get("currentWorkItemId") ?? undefined
    const queueContextId =
        searchParams.get("queueContextId") ?? `queue:card-funds-review:${scope}`

    const autoNextExplicit = searchParams.get("autoNext")
    const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
    const autoNext =
        autoNextExplicit === "0"
            ? false
            : autoNextExplicit === "1"
              ? true
              : sessionAutoNext

    const [searchInput, setSearchInput] = React.useState(q ?? "")

    // 搜索输入（q）与 URL 对齐
    React.useEffect(() => {
        setSearchInput(q ?? "")
    }, [q])

    const replaceUrl = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            const params = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") params.delete(key)
                else params.set(key, value)
            }
            // 跨 W05/W11 返回时不丢 queueContextId
            if (!params.has("queueContextId")) {
                params.set("queueContextId", queueContextId)
            }
            const qs = params.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, queueContextId, router, searchParams],
    )

    // 300ms 防抖写 URL；q/replaceUrl 入依赖保证闭包不陈旧，
    // 避免防抖期间切换 scope/type 等参数后被旧 URL 快照覆盖（D18 竞态）
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput.trim() === (q ?? "")) return
            replaceUrl({
                q: searchInput.trim() || null,
                currentWorkItemId: null,
            })
        }, 300)
        return () => globalThis.clearTimeout(handle)
    }, [q, replaceUrl, searchInput])

    const setAutoNext = React.useCallback(
        (on: boolean) => {
            setSessionAutoNext(on)
            replaceUrl({ autoNext: on ? "1" : "0" })
        },
        [replaceUrl],
    )

    return {
        scope,
        type,
        status,
        due,
        q,
        currentWorkItemId,
        queueContextId,
        autoNext,
        searchInput,
        setSearchInput,
        setAutoNext,
        replaceUrl,
        pathname,
        searchParams,
        router,
    }
}

/** 队列数据就绪后补齐 URL 默认参数；依赖队列加载结果，须在查询之后调用。 */
export function useCardFundsReviewDefaultUrlSync(args: {
    queuePending: boolean
    view: CardFundsReviewQueueView | undefined
    task: CardFundsReviewItemView | undefined
    taskCount: number
    scope: "mine" | "role_pool"
    type: "all" | "opening" | "delta"
    queueContextId: string
}): void {
    const { queuePending, view, task, taskCount, scope, type, queueContextId } =
        args
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    // URL 默认：保留 queueContextId / scope / currentWorkItemId；
    // type 默认「all」不写 URL（默认值省略，D18）
    React.useEffect(() => {
        if (queuePending || !view) return
        const hasScope = searchParams.has("scope")
        const hasType = searchParams.has("type")
        const hasItem = searchParams.has("currentWorkItemId")
        const hasCtx = searchParams.has("queueContextId")
        if (hasScope && hasType && hasCtx && (hasItem || taskCount === 0))
            return
        const params = new URLSearchParams(searchParams.toString())
        if (!hasScope) params.set("scope", scope)
        if (!hasType && type !== "all") params.set("type", type)
        if (!hasCtx) params.set("queueContextId", queueContextId)
        if (!hasItem && task) {
            params.set("currentWorkItemId", task.workItem.workItemId)
        }
        const qs = params.toString()
        if (qs === searchParams.toString()) return
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    }, [
        queuePending,
        view,
        searchParams,
        scope,
        type,
        queueContextId,
        task,
        taskCount,
        pathname,
        router,
    ])
}
