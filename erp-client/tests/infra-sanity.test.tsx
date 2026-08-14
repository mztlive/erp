import { QueryClientProvider, useQuery } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { useCallback, useState } from 'react'
import { describe, expect, it } from 'vitest'

import { createFreshQueryClient, renderHookWithProviders } from '@/features/test-utils'

function useCounter() {
    const [count, setCount] = useState(0)
    const increment = useCallback(() => setCount((current) => current + 1), [])
    return { count, increment }
}

function useGreeting() {
    return useQuery({
        queryKey: ['greeting'],
        queryFn: async () => 'hello',
    })
}

describe('test infra sanity', () => {
    it('renders a plain hook with renderHook', () => {
        const { result } = renderHook(() => useCounter())
        expect(result.current.count).toBe(0)
        act(() => result.current.increment())
        expect(result.current.count).toBe(1)
    })

    it('renders a plain hook with renderHookWithProviders', () => {
        const { result } = renderHookWithProviders(() => useCounter())
        expect(result.current.count).toBe(0)
        act(() => result.current.increment())
        expect(result.current.count).toBe(1)
    })

    it('renders a query hook with renderHook', async () => {
        const client = createFreshQueryClient()
        const wrapper = ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        )
        const { result } = renderHook(() => useGreeting(), { wrapper })
        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.data).toBe('hello'))
        expect(result.current.isSuccess).toBe(true)
    })

    it('renders a query hook with renderHookWithProviders', async () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(() => useGreeting(), {
            queryClient: client,
        })
        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.data).toBe('hello'))
        expect(result.current.isSuccess).toBe(true)
    })
})
