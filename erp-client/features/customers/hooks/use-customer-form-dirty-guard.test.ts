import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

import { useCustomerFormDirtyGuard } from './use-customer-form-dirty-guard'

type UnloadHandler = (e: BeforeUnloadEvent) => void

function beforeunloadHandler(
    calls: Parameters<typeof window.addEventListener>[],
): UnloadHandler | undefined {
    return calls.find(([type]) => type === 'beforeunload')?.[1] as
        | UnloadHandler
        | undefined
}

describe('useCustomerFormDirtyGuard', () => {
    let addSpy: ReturnType<typeof vi.spyOn>
    let removeSpy: ReturnType<typeof vi.spyOn>

    beforeEach(() => {
        vi.clearAllMocks()
        addSpy = vi.spyOn(window, 'addEventListener')
        removeSpy = vi.spyOn(window, 'removeEventListener')
    })

    it('registers a beforeunload blocker only while the form is dirty', () => {
        const { rerender } = renderHook(
            ({ dirty }: { dirty: boolean }) => useCustomerFormDirtyGuard(dirty),
            { initialProps: { dirty: false } },
        )

        expect(beforeunloadHandler(addSpy.mock.calls)).toBeUndefined()

        rerender({ dirty: true })
        const handler = beforeunloadHandler(addSpy.mock.calls)
        expect(handler).toBeTypeOf('function')

        rerender({ dirty: false })
        expect(removeSpy).toHaveBeenCalledWith('beforeunload', handler)
    })

    it('blocks unload with the unsaved-input message when dirty', () => {
        renderHook(() => useCustomerFormDirtyGuard(true))

        const handler = beforeunloadHandler(addSpy.mock.calls)
        expect(handler).toBeTypeOf('function')
        const event = {
            preventDefault: vi.fn(),
            returnValue: '',
        } as unknown as BeforeUnloadEvent
        handler!(event)

        expect(event.preventDefault).toHaveBeenCalledTimes(1)
        expect(event.returnValue).toBe('当前输入尚未提交，刷新后将丢失。')
    })

    it('cleans up the listener on unmount while dirty', () => {
        const { unmount } = renderHook(() => useCustomerFormDirtyGuard(true))

        const handler = beforeunloadHandler(addSpy.mock.calls)
        unmount()

        expect(removeSpy).toHaveBeenCalledWith('beforeunload', handler)
    })
})
