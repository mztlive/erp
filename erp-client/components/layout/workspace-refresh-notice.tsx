"use client"

import { useEffect, useRef, useState } from "react"
import {
    focusManager,
    onlineManager,
    useQueryClient,
} from "@tanstack/react-query"
import { reportProblem, resolveProblem } from "@/lib/system-feedback"
import { createApiError } from "@/lib/api/errors"
import { workspaceFormState } from "@/lib/workspace-form-guard"
import { workspaceHost } from "@/lib/workspace-tabs"

type RefreshState =
    | { status: "idle" | "deferred" | "refreshing" }
    | { status: "error"; error: unknown }

/** 通知保留至刷新成功；隐藏、编辑和提交中的页签延后读取，不重建页面或表单。 */
function useWorkspaceRefresh() {
    const queryClient = useQueryClient()
    const [state, setState] = useState<RefreshState>({ status: "idle" })
    const retry = useRef<() => Promise<void>>(async () => {})

    useEffect(() => {
        let receivedRevision = 0
        let appliedRevision = 0
        let refreshing = false
        let disposed = false
        let active =
            window.frameElement?.getAttribute("data-workspace-active") ===
            "true"

        const refresh = async (includeStale = false) => {
            if (disposed || refreshing) return
            const pending = receivedRevision > appliedRevision
            const form = workspaceFormState()
            const guarded =
                form.dirty || form.busy || queryClient.isMutating() > 0
            const visible = active && document.visibilityState === "visible"
            focusManager.setFocused(visible && !guarded)
            if (!visible) return
            if (guarded) {
                if (pending) setState({ status: "deferred" })
                return
            }
            if (!pending && !includeStale) return

            refreshing = true
            const revision = receivedRevision
            if (pending) setState({ status: "refreshing" })
            try {
                // 前一轮请求结束可能清掉更新通知写入的过期标记，每轮都重新标记。
                if (pending)
                    await queryClient.invalidateQueries({ refetchType: "none" })
                await queryClient.refetchQueries(
                    { type: "active", stale: true },
                    { throwOnError: true },
                )
                // TanStack Query 对暂停的请求立即返回，不能把等待网络当成刷新成功。
                if (
                    queryClient.getQueryCache().findAll({
                        type: "active",
                        predicate: (query) =>
                            query.state.fetchStatus === "paused",
                    }).length > 0
                )
                    throw createApiError({
                        kind: "Network",
                        message: "网络连接暂不可用，请恢复连接后重试。",
                        retryable: true,
                    })
                if (disposed) return
                appliedRevision = revision
                if (pending) workspaceHost()?.refreshed(window, revision)
                if (receivedRevision === revision) setState({ status: "idle" })
            } catch (error) {
                if (!disposed) setState({ status: "error", error })
            } finally {
                refreshing = false
                // 刷新期间收到新通知时补一轮；失败的同一版本留给显式重试或再次激活。
                if (!disposed && receivedRevision > revision) void refresh()
            }
        }

        const activate = (value: boolean) => {
            active = value
            if (!active) focusManager.setFocused(false)
            void refresh(true)
        }
        const visibility = () =>
            activate(
                window.frameElement?.getAttribute("data-workspace-active") ===
                    "true",
            )
        const formChanged = () => void refresh()
        window.erpWorkspaceActivate = activate
        window.erpWorkspaceInvalidate = (revision) => {
            if (revision <= receivedRevision) return
            receivedRevision = revision
            void queryClient.invalidateQueries({ refetchType: "none" })
            void refresh()
        }
        retry.current = () => refresh(true)
        const unsubscribeMutation = queryClient
            .getMutationCache()
            .subscribe((event) => {
                if (
                    event.type === "updated" &&
                    (event.action.type === "success" ||
                        event.action.type === "error")
                )
                    void refresh()
            })
        const unsubscribeOnline = onlineManager.subscribe((online) => {
            if (online) void refresh(true)
        })
        document.addEventListener("visibilitychange", visibility)
        window.addEventListener("erp-workspace-form-state", formChanged)
        visibility()
        return () => {
            disposed = true
            retry.current = async () => {}
            unsubscribeMutation()
            unsubscribeOnline()
            document.removeEventListener("visibilitychange", visibility)
            window.removeEventListener("erp-workspace-form-state", formChanged)
            delete window.erpWorkspaceActivate
            delete window.erpWorkspaceInvalidate
            focusManager.setFocused(undefined)
        }
    }, [queryClient])

    return { state, retry }
}

/** 刷新不占据页面顶部；失败统一进入系统提示，保留已有内容与输入。 */
export function WorkspaceRefreshNotice() {
    const { state, retry } = useWorkspaceRefresh()
    useEffect(() => {
        if (state.status === "error") {
            // 请求错误已由 QueryCache 汇总；这里只补充断网等非请求错误。
            if (
                state.error instanceof Error &&
                state.error.message === "网络连接暂不可用，请恢复连接后重试。"
            ) {
                reportProblem("workspace-refresh", state.error, {
                    title: "页面数据尚未刷新",
                    description: "网络连接暂不可用，当前显示上次加载的结果。",
                    retry: async () => {
                        await retry.current()
                    },
                })
            }
        } else if (state.status === "idle") resolveProblem("workspace-refresh")
    }, [state, retry])
    return null
}
