import { cleanup, fireEvent, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { useSettlementListSearchHotkey } from './use-settlement-list-search-hotkey'

afterEach(() => {
    cleanup()
    document.body.innerHTML = ''
})

function setupSearchInput(): HTMLInputElement {
    const input = document.createElement('input')
    input.setAttribute('data-slot', 'settlement-list-search')
    document.body.appendChild(input)
    return input
}

describe('useSettlementListSearchHotkey', () => {
    it('focuses the settlement search input when "/" is pressed outside inputs', () => {
        const input = setupSearchInput()
        renderHook(() => useSettlementListSearchHotkey())

        fireEvent.keyDown(window, { key: '/' })

        expect(document.activeElement).toBe(input)
    })

    it('does not steal focus when a modifier key is held', () => {
        const input = setupSearchInput()
        renderHook(() => useSettlementListSearchHotkey())

        fireEvent.keyDown(window, { key: '/', metaKey: true })
        expect(document.activeElement).not.toBe(input)
        fireEvent.keyDown(window, { key: '/', ctrlKey: true })
        expect(document.activeElement).not.toBe(input)
        fireEvent.keyDown(window, { key: '/', altKey: true })
        expect(document.activeElement).not.toBe(input)
    })

    it('ignores other keys', () => {
        const input = setupSearchInput()
        renderHook(() => useSettlementListSearchHotkey())

        fireEvent.keyDown(window, { key: 'Enter' })
        expect(document.activeElement).not.toBe(input)
    })

    it('ignores the shortcut while typing inside an input or textarea', () => {
        const search = setupSearchInput()
        const other = document.createElement('input')
        document.body.appendChild(other)
        renderHook(() => useSettlementListSearchHotkey())

        other.focus()
        fireEvent.keyDown(other, { key: '/' })
        expect(document.activeElement).not.toBe(search)
        expect(document.activeElement).toBe(other)

        const textarea = document.createElement('textarea')
        document.body.appendChild(textarea)
        textarea.focus()
        fireEvent.keyDown(textarea, { key: '/' })
        expect(document.activeElement).not.toBe(search)
    })

    it('stops listening after unmount', () => {
        const input = setupSearchInput()
        const { unmount } = renderHook(() => useSettlementListSearchHotkey())

        unmount()
        fireEvent.keyDown(window, { key: '/' })

        expect(document.activeElement).not.toBe(input)
    })
})
