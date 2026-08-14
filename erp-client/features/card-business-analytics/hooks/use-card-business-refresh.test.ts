import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useCardBusinessRefresh } from './use-card-business-refresh'

describe('useCardBusinessRefresh', () => {
    let refetch: ReturnType<typeof vi.fn<() => Promise<unknown>>>

    beforeEach(() => {
        refetch = vi.fn<() => Promise<unknown>>()
    })

    it('starts idle and clears after a successful refresh', async () => {
        refetch.mockResolvedValue({ isSuccess: true })
        const { result } = renderHook(() => useCardBusinessRefresh(refetch))
        expect(result.current.refreshing).toBe(false)
        expect(result.current.refreshFailed).toBeNull()

        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(result.current.refreshing).toBe(false)
        expect(result.current.refreshFailed).toBeNull()
    })

    it('flags refreshing while the refetch is in flight', async () => {
        let resolveRefetch: () => void = () => {}
        refetch.mockImplementation(
            () =>
                new Promise<void>((resolve) => {
                    resolveRefetch = resolve
                }),
        )
        const { result } = renderHook(() => useCardBusinessRefresh(refetch))
        act(() => {
            void result.current.handleRefresh()
        })
        await waitFor(() => expect(result.current.refreshing).toBe(true))
        act(() => {
            resolveRefetch()
        })
        await waitFor(() => expect(result.current.refreshing).toBe(false))
    })

    it('records the error message and resets refreshing on failure', async () => {
        refetch.mockRejectedValue(new Error('network down'))
        const { result } = renderHook(() => useCardBusinessRefresh(refetch))
        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(result.current.refreshing).toBe(false)
        expect(result.current.refreshFailed).toBe('network down')
    })

    it('uses the fallback message for errors without a readable message', async () => {
        refetch.mockRejectedValue({ kind: 'Validation', status: 400 })
        const { result } = renderHook(() => useCardBusinessRefresh(refetch))
        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(result.current.refreshing).toBe(false)
        expect(result.current.refreshFailed).toBe(
            '刷新失败，已保留上次成功数据。',
        )
    })

    it('clears a previous failure on the next attempt', async () => {
        refetch.mockRejectedValueOnce(new Error('first failure'))
        refetch.mockResolvedValueOnce({ isSuccess: true })
        const { result } = renderHook(() => useCardBusinessRefresh(refetch))
        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(result.current.refreshFailed).toBe('first failure')
        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(result.current.refreshFailed).toBeNull()
    })
})
