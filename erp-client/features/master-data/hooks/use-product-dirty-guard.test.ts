import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook } from '@testing-library/react'

import { useProductDirtyGuard } from './use-product-dirty-guard'

describe('useProductDirtyGuard', () => {
    let addSpy: ReturnType<typeof vi.spyOn>
    let removeSpy: ReturnType<typeof vi.spyOn>

    beforeEach(() => {
        addSpy = vi.spyOn(window, 'addEventListener')
        removeSpy = vi.spyOn(window, 'removeEventListener')
    })

    afterEach(() => {
        vi.restoreAllMocks()
    })

    function registeredHandler() {
        const calls = addSpy.mock
            .calls as [string, EventListenerOrEventListenerObject][]
        const call = calls.find(([name]) => name === 'beforeunload')
        return call?.[1] as ((event: BeforeUnloadEvent) => void) | undefined
    }

    it('registers a beforeunload listener exactly once on mount', () => {
        let dirty = false
        const { rerender } = renderHook(() =>
            useProductDirtyGuard(() => dirty),
        )

        expect(
            addSpy.mock.calls.filter(
                ([name]: [string, EventListenerOrEventListenerObject]) =>
                    name === 'beforeunload',
            ),
        ).toHaveLength(1)

        dirty = true
        rerender()
        expect(
            addSpy.mock.calls.filter(
                ([name]: [string, EventListenerOrEventListenerObject]) =>
                    name === 'beforeunload',
            ),
        ).toHaveLength(1)
    })

    it('prevents unload only when the form is dirty', () => {
        let dirty = false
        renderHook(() => useProductDirtyGuard(() => dirty))
        const handler = registeredHandler()
        expect(handler).toBeDefined()

        const cleanEvent = new Event('beforeunload', { cancelable: true })
        handler!(cleanEvent as BeforeUnloadEvent)
        expect(cleanEvent.defaultPrevented).toBe(false)

        dirty = true
        const dirtyEvent = new Event('beforeunload', { cancelable: true })
        handler!(dirtyEvent as BeforeUnloadEvent)
        expect(dirtyEvent.defaultPrevented).toBe(true)
    })

    it('removes the listener on unmount', () => {
        const { unmount } = renderHook(() => useProductDirtyGuard(() => false))

        unmount()
        expect(
            removeSpy.mock.calls.filter(
                ([name]: [string, EventListenerOrEventListenerObject]) =>
                    name === 'beforeunload',
            ),
        ).toHaveLength(1)
    })
})
