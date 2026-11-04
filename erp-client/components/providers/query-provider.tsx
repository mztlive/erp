"use client"

import { useEffect, useState } from "react"
import { QueryClientProvider } from "@tanstack/react-query"
import { ReactQueryDevtools } from "@tanstack/react-query-devtools"
import { toast } from "@/components/ui/toast"
import { feedbackStore, reportProblem, problemKey } from "@/lib/system-feedback"
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
    reportProblem(`operation:${failure.code ?? failure.title}`, error, {
        title: "操作未完成",
        operation: true,
        description:
            failure.kind === "system"
                ? "暂未确认操作结果，请回到来源页面核对；当前输入已保留。"
                : failure.kind === "permission"
                  ? "当前账号无权执行此操作，请联系管理员确认权限。"
                  : failure.description,
    })
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
            if (event.type === "removed") {
                store.resolve(reportedKeys.get(key) ?? problemKey(key))
                reportedKeys.delete(key)
                return
            }
            if (event.type !== "updated") return
            if (event.action.type === "success") {
                store.resolve(reportedKeys.get(key) ?? problemKey(key))
                reportedKeys.delete(key)
            }
            if (event.action.type === "error" && event.query.isActive()) {
                const stale = event.query.state.data !== undefined
                const denied =
                    getErrorPresentation(event.query.state.error).kind ===
                    "permission"
                const previousKey = reportedKeys.get(key)
                if (previousKey && previousKey !== problemKey(key))
                    store.resolve(previousKey)
                reportedKeys.set(key, problemKey(key))
                reportProblem(key, event.query.state.error, {
                    title: denied
                        ? "部分资料无权查看"
                        : stale
                          ? "部分数据刷新失败"
                          : "部分资料未能加载",
                    description: denied
                        ? undefined
                        : stale
                          ? "当前显示上次加载的结果，重新加载成功后会自动更新。"
                          : undefined,
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
