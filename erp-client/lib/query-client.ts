import { MutationCache, QueryClient } from "@tanstack/react-query"

export type MutationErrorHandler = (error: unknown) => void

/**
 * SPA 专用 QueryClient 工厂。
 * 所有服务端数据请求必须经由 TanStack Query（useQuery / useMutation / useInfiniteQuery 等）。
 */
export const makeQueryClient = (onMutationError?: MutationErrorHandler) => {
    return new QueryClient({
        mutationCache: new MutationCache({
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
}
