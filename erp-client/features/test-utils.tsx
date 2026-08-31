import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook } from "@testing-library/react"
import type { ReactNode } from "react"

export function createFreshQueryClient(): QueryClient {
    return new QueryClient({
        defaultOptions: {
            queries: { retry: false },
            mutations: { retry: false },
        },
    })
}

export function renderHookWithProviders<Result, Props>(
    callback: (initialProps: Props) => Result,
    options?: { queryClient?: QueryClient },
) {
    const client = options?.queryClient ?? createFreshQueryClient()
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
    return renderHook(callback, { wrapper })
}
