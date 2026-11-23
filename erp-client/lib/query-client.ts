import { MutationCache, QueryClient } from "@tanstack/react-query"
import type { ReadProblemContext } from "@/lib/system-feedback"

declare module "@tanstack/react-query" {
    interface Register {
        queryMeta: Record<string, unknown> & {
            feedback?: ReadProblemContext
            taskMaterialWorkItemId?: string
        }
    }
}

export type MutationErrorHandler = (error: unknown) => void

/**
 * SPA 专用 QueryClient 工厂。
 * 所有服务端数据请求必须经由 TanStack Query（useQuery / useMutation / useInfiniteQuery 等）。
 */
export const makeQueryClient = (
    onMutationError?: MutationErrorHandler,
    onMutationSuccess?: () => void,
) => {
    const client = new QueryClient({
        mutationCache: new MutationCache({
            onSuccess: () => {
                // 全局成功回调先于业务回调；必须在业务跳转或关闭页签前交付通知。
                onMutationSuccess?.()
                void client.invalidateQueries({
                    predicate: (query) =>
                        query.meta?.businessTimeline === true ||
                        query.meta?.roleWorkbench === true ||
                        query.meta?.workspaceNavigation === true,
                })
            },
            onError: (error, _variables, _context, mutation) => {
                if (mutation.meta?.suppressErrorToast === true) return
                onMutationError?.(error)
            },
        }),
        defaultOptions: {
            queries: {
                // SPA：数据在客户端获取与缓存，窗口重新聚焦时默认不强制刷新
                staleTime: 60 * 1000,
                refetchOnWindowFocus: false,
                retry: 1,
            },
            mutations: {
                retry: 0,
            },
        },
    })
    return client
}
