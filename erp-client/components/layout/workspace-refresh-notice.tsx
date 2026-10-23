"use client"

import { useEffect, useRef, useState } from "react"
import {
    focusManager,
    onlineManager,
    useQueryClient,
} from "@tanstack/react-query"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { getErrorPresentation } from "@/lib/api/errors"
import { workspaceFormState } from "@/lib/workspace-form-guard"
import { workspaceHost } from "@/lib/workspace-tabs"

type RefreshState =
    | { status: "idle" | "deferred" | "refreshing" }
    | { status: "error"; error: unknown }

/** 通知保留至刷新成功；隐藏、编辑和提交中的页签延后读取，不重建页面或表单。 */
function useWorkspaceRefresh() {
    const queryClient = useQueryClient()
    const [state, setState] = useState<RefreshState>({ status: "idle" })
    const retry = useRef<() => void>(() => {})

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
                    throw new Error("网络连接暂不可用，请恢复连接后重试。")
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
        retry.current = () => void refresh(true)
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
            retry.current = () => {}
            unsubscribeMutation()
            unsubscribeOnline()
            document.removeEventListener("visibilitychange", visibility)
            window.removeEventListener("erp-workspace-form-state", formChanged)
            delete window.erpWorkspaceActivate
            delete window.erpWorkspaceInvalidate
            focusManager.setFocused(undefined)
        }
    }, [queryClient])

    return { state, retry: () => retry.current() }
}

/** 保留原页面数据，提示延后刷新及失败原因；重试只读取数据，不重复提交业务动作。 */
export function WorkspaceRefreshNotice() {
    const { state, retry } = useWorkspaceRefresh()
    if (state.status === "idle") return null
    const failure =
        state.status === "error" ? getErrorPresentation(state.error) : undefined
    return (
        <div className="shrink-0 px-3 pt-3">
            <Alert variant={failure ? "warning" : "info"}>
                <AlertTitle>
                    {failure
                        ? "页面数据刷新失败"
                        : state.status === "deferred"
                          ? "相关数据已更新"
                          : "正在更新页面数据…"}
                </AlertTitle>
                <AlertDescription>
                    {failure ? (
                        <div className="flex flex-wrap items-center gap-3">
                            <span>
                                {failure.description}{" "}
                                当前显示的数据可能不是最新结果。
                                {failure.code && ` 错误码：${failure.code}`}
                                {failure.requestId &&
                                    ` 请求编号：${failure.requestId}`}
                            </span>
                            <Button
                                id="workspace-frame-refresh-retry"
                                variant="outline"
                                size="sm"
                                onClick={retry}
                            >
                                重试刷新
                            </Button>
                        </div>
                    ) : state.status === "deferred" ? (
                        "为保留当前输入，将在保存、取消编辑或提交结束后自动刷新。"
                    ) : (
                        "更新完成前保留当前页面内容。"
                    )}
                </AlertDescription>
            </Alert>
        </div>
    )
}
