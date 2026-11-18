"use client"

import { useEffect, useState } from "react"
import { QueryClientProvider } from "@tanstack/react-query"
import { ReactQueryDevtools } from "@tanstack/react-query-devtools"
import { toast } from "@/components/ui/toast"
import {
    feedbackStore,
    reportProblem,
    reportOperationProblem,
    problemKey,
} from "@/lib/system-feedback"
import { fieldErrorMessages } from "@/lib/api/field-errors"
import { getErrorPresentation } from "@/lib/api/errors"
import { subscribeScopeCache } from "@/features/data-scope/cache"
import { trackNavDeliveryOrigin } from "@/lib/nav-delivery"
import { makeQueryClient } from "@/lib/query-client"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

/** 父页面先记录变更，后续业务回调关闭来源页签也不影响交付。 */
const notifyMutationSuccess = () => {
    if (isWorkspaceFrame()) workspaceHost()?.changed(window)
}

/** 将 Mutation 接口失败统一展示为短时提示。 */
const notifyMutationError = (error: unknown) => {
    // 验证失败继续交给字段定位；网络异常不能诱导重复提交。
    const failure = getErrorPresentation(error)
    if (!isWorkspaceFrame() && !window.erpWorkspaceHost) {
        toast.add({
            title: failure.title,
            description: [
                failure.description,
                ...fieldErrorMessages(failure.fieldErrors),
            ].join("\n"),
            type: "error",
            timeout: 6000,
        })
        return
    }
    reportOperationProblem(error)
}

/**
 * 全局 TanStack Query Provider。
 * 本项目是纯 SPA：所有网络请求必须通过 useQuery / useMutation / useInfiniteQuery 等，
 * 禁止在组件内直接 fetch/axios 后自管 loading/error，禁止 Server Components 拉数。
 */
export function QueryProvider({ children }: { children: React.ReactNode }) {
    // 客户端单例：避免每次 render 新建 QueryClient 导致缓存丢失
    const [queryClient] = useState(() =>
        makeQueryClient(notifyMutationError, notifyMutationSuccess),
    )

    useEffect(() => {
        const store = feedbackStore()
        const reportedKeys = new Map<string, string>()
        const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
            const key = `query:${event.query.queryHash}`
            if (
                event.type === "removed" ||
                ((event.type === "observerRemoved" ||
                    event.type === "observerOptionsUpdated") &&
                    !event.query.isActive())
            ) {
                store.resolve(reportedKeys.get(key) ?? problemKey(key))
                reportedKeys.delete(key)
                return
            }
            if (event.type === "updated" && event.action.type === "success") {
                store.resolve(reportedKeys.get(key) ?? problemKey(key))
                reportedKeys.delete(key)
            }
            if (
                ((event.type === "updated" && event.action.type === "error") ||
                    (event.type === "observerAdded" &&
                        event.query.state.status === "error")) &&
                event.query.isActive()
            ) {
                const previousKey = reportedKeys.get(key)
                if (previousKey && previousKey !== problemKey(key))
                    store.resolve(previousKey)
                reportedKeys.set(key, problemKey(key))
                reportProblem(key, event.query.state.error, {
                    readContext: event.query.meta?.feedback,
                    refreshFailed: event.query.state.data !== undefined,
                    retry: () => event.query.fetch(),
                })
            }
        })
        return () => {
            unsubscribe()
            store.forgetSource(window)
        }
    }, [queryClient])

    useEffect(() => subscribeScopeCache(queryClient), [queryClient])
    useEffect(trackNavDeliveryOrigin, [])

    return (
        <QueryClientProvider client={queryClient}>
            {children}
            {process.env.NODE_ENV === "development" ? (
                <ReactQueryDevtools
                    initialIsOpen={false}
                    buttonPosition="bottom-left"
                />
            ) : null}
        </QueryClientProvider>
    )
}
