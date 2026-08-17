"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { buildReturnHref } from "@/features/procurement-confirmation/lib/urls"

export type ProcurementConfirmationScope = "mine" | "team"
export type ProcurementConfirmationDue = "active" | "today" | "overdue"
export type ProcurementConfirmationSort = "due_at" | "submitted_at" | "priority"

/**
 * W07 页面 URL 参数解析与写入：
 * scope / due / sort / orderNo / currentWorkItemId / queueContextId / autoNext，
 * 以及单号搜索草稿（防抖写 URL）、清除筛选与自动下一项偏好。
 */
export function useProcurementConfirmationUrl() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const scope: ProcurementConfirmationScope =
        searchParams.get("scope") === "team" ? "team" : "mine"
    const dueParam = searchParams.get("due")
    const due: ProcurementConfirmationDue =
        dueParam === "today" || dueParam === "overdue" || dueParam === "active"
            ? dueParam
            : "active"
    const sortParam = searchParams.get("sort")
    const sort: ProcurementConfirmationSort =
        sortParam === "submitted_at" || sortParam === "priority"
            ? sortParam
            : "due_at"
    const orderNo = searchParams.get("orderNo") ?? undefined
    const currentWorkItemId =
        searchParams.get("currentWorkItemId") ??
        searchParams.get("task") ??
        undefined
    const queueContextId =
        searchParams.get("queueContextId") ??
        `queue:procurement-confirmation:${scope}`

    // autoNext：显式 URL 优先；否则会话默认 true；不写 localStorage
    const autoNextExplicit = searchParams.get("autoNext")
    const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
    const autoNext =
        autoNextExplicit === "0"
            ? false
            : autoNextExplicit === "1"
              ? true
              : sessionAutoNext

    /** 单号搜索草稿：输入过程不打 URL，防抖/回车才提交 */
    const [orderNoDraft, setOrderNoDraft] = React.useState(orderNo ?? "")
    const orderNoInputRef = React.useRef<HTMLInputElement>(null)

    // 单号搜索框与 URL 双向同步：后退/分享后输入框与结果保持一致
    React.useEffect(() => {
        setOrderNoDraft(orderNo ?? "")
    }, [orderNo])

    const replaceUrl = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            const params = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") params.delete(key)
                else params.set(key, value)
            }
            params.delete("task")
            params.delete("completed")
            const qs = params.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    // 单号搜索：300ms 防抖写 URL（replace）；筛选变化时清空焦点
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (orderNoDraft.trim() === (orderNo ?? "")) return
            replaceUrl({
                orderNo: orderNoDraft.trim() || null,
                currentWorkItemId: null,
            })
        }, 300)
        return () => globalThis.clearTimeout(handle)
    }, [orderNo, orderNoDraft, replaceUrl])

    const commitOrderNo = React.useCallback(() => {
        if (orderNoDraft.trim() === (orderNo ?? "")) return
        replaceUrl({
            orderNo: orderNoDraft.trim() || null,
            currentWorkItemId: null,
        })
    }, [orderNo, orderNoDraft, replaceUrl])

    // scope/sort 不算筛选；due/orderNo 才算激活筛选
    const hasActiveFilter = Boolean(
        orderNo || dueParam === "today" || dueParam === "overdue",
    )

    // 清除筛选：清 orderNo/due + 焦点，保留 scope/sort/queueContextId
    const clearFilters = React.useCallback(() => {
        setOrderNoDraft("")
        replaceUrl({ orderNo: null, due: null, currentWorkItemId: null })
    }, [replaceUrl])

    const toggleAutoNext = React.useCallback(
        (next: boolean) => {
            // preferenceScope 未配置：只写显式 URL / 会话，不写 localStorage
            setSessionAutoNext(next)
            replaceUrl({ autoNext: next ? "1" : "0" })
        },
        [replaceUrl],
    )

    const handleScopeChange = React.useCallback(
        (nextScope: ProcurementConfirmationScope) => {
            replaceUrl({
                scope: nextScope === "mine" ? null : nextScope,
                queueContextId: null,
                currentWorkItemId: null,
            })
        },
        [replaceUrl],
    )

    const handleDueChange = React.useCallback(
        (nextDue: ProcurementConfirmationDue) => {
            replaceUrl({
                due: nextDue === "active" ? null : nextDue,
                currentWorkItemId: null,
            })
        },
        [replaceUrl],
    )

    const returnTo = buildReturnHref(
        new URLSearchParams(searchParams.toString()),
    )

    return {
        scope,
        due,
        sort,
        orderNo,
        currentWorkItemId,
        queueContextId,
        autoNext,
        orderNoDraft,
        setOrderNoDraft,
        orderNoInputRef,
        commitOrderNo,
        hasActiveFilter,
        clearFilters,
        toggleAutoNext,
        handleScopeChange,
        handleDueChange,
        replaceUrl,
        returnTo,
    }
}

export type ProcurementConfirmationQueueUrlSyncOptions = {
    scope: ProcurementConfirmationScope
    queueContextId: string
    /** 队列已取得数据（!isPending && !!view） */
    queueReady: boolean
    tasksLength: number
    currentTaskWorkItemId: string | undefined
}

/**
 * 默认 URL 补齐：scope / currentWorkItemId / queueContextId
 * （不写 autoNext 除非用户切换）；兼容旧 task= 参数迁移。
 */
export function useProcurementConfirmationQueueUrlSync({
    scope,
    queueContextId,
    queueReady,
    tasksLength,
    currentTaskWorkItemId,
}: ProcurementConfirmationQueueUrlSyncOptions) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    React.useEffect(() => {
        if (!queueReady) return
        const hasScope = searchParams.has("scope")
        const hasItem =
            searchParams.has("currentWorkItemId") || searchParams.has("task")
        const hasCtx = searchParams.has("queueContextId")
        if (hasScope && hasCtx && (hasItem || tasksLength === 0)) return
        const params = new URLSearchParams(searchParams.toString())
        if (!hasScope) params.set("scope", scope)
        if (!hasCtx) params.set("queueContextId", queueContextId)
        if (!hasItem && currentTaskWorkItemId) {
            params.set("currentWorkItemId", currentTaskWorkItemId)
            params.delete("task")
        }
        // 兼容旧 task= 参数：迁移到 currentWorkItemId
        if (searchParams.has("task") && currentTaskWorkItemId) {
            params.set("currentWorkItemId", currentTaskWorkItemId)
            params.delete("task")
        }
        params.delete("completed")
        const qs = params.toString()
        const next = qs ? `${pathname}?${qs}` : pathname
        router.replace(next, { scroll: false })
    }, [
        queueReady,
        searchParams,
        scope,
        queueContextId,
        currentTaskWorkItemId,
        tasksLength,
        pathname,
        router,
    ])
}
