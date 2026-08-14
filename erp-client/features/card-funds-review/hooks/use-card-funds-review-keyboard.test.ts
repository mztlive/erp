import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act, cleanup } from '@testing-library/react'

import { useCardFundsReviewKeyboard } from './use-card-funds-review-keyboard'
import { makeTask } from './test-data'

function pressKey(init: KeyboardEventInit, target?: EventTarget) {
    const event = new KeyboardEvent('keydown', init)
    ;(target ?? window).dispatchEvent(event)
    return event
}

describe('useCardFundsReviewKeyboard', () => {
    const task = makeTask()
    const neighborId = vi.fn<(delta: number) => string | undefined>((delta) =>
        delta > 0 ? 'wi_2' : 'wi_0',
    )
    const goToWorkItem = vi.fn()
    const onShortcutSubmit = vi.fn()
    const setPendingNav = vi.fn()

    function renderKeyboard(overrides: Record<string, unknown> = {}) {
        return renderHook(
            () =>
                useCardFundsReviewKeyboard({
                    task,
                    evidenceOk: true,
                    evidenceDirty: false,
                    neighborId,
                    goToWorkItem,
                    onShortcutSubmit,
                    setPendingNav,
                    ...overrides,
                }),
        )
    }

    beforeEach(() => {
        vi.clearAllMocks()
    })

    afterEach(() => {
        cleanup()
    })

    it('navigates to the next/previous work item on j/k and arrow keys', () => {
        renderKeyboard()
        act(() => {
            pressKey({ key: 'j' })
        })
        expect(neighborId).toHaveBeenCalledWith(1)
        expect(goToWorkItem).toHaveBeenCalledWith('wi_2')

        act(() => {
            pressKey({ key: 'k' })
        })
        expect(neighborId).toHaveBeenCalledWith(-1)
        expect(goToWorkItem).toHaveBeenCalledWith('wi_0')

        act(() => {
            pressKey({ key: 'ArrowDown' })
        })
        expect(goToWorkItem).toHaveBeenLastCalledWith('wi_2')

        act(() => {
            pressKey({ key: 'ArrowUp' })
        })
        expect(goToWorkItem).toHaveBeenLastCalledWith('wi_0')
    })

    it('stays put when there is no neighbor', () => {
        neighborId.mockImplementationOnce(() => undefined)
        renderKeyboard()
        act(() => {
            pressKey({ key: 'j' })
        })
        expect(goToWorkItem).not.toHaveBeenCalled()
        expect(setPendingNav).not.toHaveBeenCalled()
    })

    it('defers navigation to the discard confirmation when evidence is dirty', () => {
        renderKeyboard({ evidenceDirty: true })
        act(() => {
            pressKey({ key: 'j' })
        })
        expect(goToWorkItem).not.toHaveBeenCalled()
        expect(setPendingNav).toHaveBeenCalledWith(1)

        act(() => {
            pressKey({ key: 'k' })
        })
        expect(setPendingNav).toHaveBeenCalledWith(-1)
    })

    it('opens the shortcut submit on meta/ctrl+Enter', () => {
        renderKeyboard()
        act(() => {
            pressKey({ key: 'Enter', metaKey: true })
        })
        expect(onShortcutSubmit).toHaveBeenCalledTimes(1)

        act(() => {
            pressKey({ key: 'Enter', ctrlKey: true })
        })
        expect(onShortcutSubmit).toHaveBeenCalledTimes(2)
    })

    it('ignores shortcut and navigation inside form fields', () => {
        renderKeyboard()
        const input = document.createElement('input')
        document.body.appendChild(input)
        act(() => {
            pressKey({ key: 'Enter', metaKey: true }, input)
            pressKey({ key: 'j' }, input)
        })
        expect(onShortcutSubmit).not.toHaveBeenCalled()
        expect(goToWorkItem).not.toHaveBeenCalled()
        input.remove()
    })

    it('removes the keydown listener on unmount', () => {
        const { unmount } = renderKeyboard()
        unmount()
        act(() => {
            pressKey({ key: 'j' })
        })
        expect(goToWorkItem).not.toHaveBeenCalled()
    })
})
