import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { usePhoneNarrow } from './use-phone-narrow'

type MatchMediaListener = (event: { matches: boolean }) => void

function stubMatchMedia(initial: boolean) {
    let matches = initial
    const listeners = new Set<MatchMediaListener>()
    const mql = {
        get matches() {
            return matches
        },
        media: '(max-width: 480px)',
        addEventListener: (_type: string, listener: MatchMediaListener) => {
            listeners.add(listener)
        },
        removeEventListener: (
            _type: string,
            listener: MatchMediaListener,
        ) => {
            listeners.delete(listener)
        },
    }
    vi.stubGlobal('matchMedia', vi.fn(() => mql))
    return {
        setMatches: (next: boolean) => {
            matches = next
        },
        listeners,
    }
}

afterEach(() => {
    vi.unstubAllGlobals()
})

describe('usePhoneNarrow', () => {
    it('returns false when the viewport is wider than 480px', () => {
        stubMatchMedia(false)
        const { result } = renderHook(() => usePhoneNarrow())
        expect(result.current).toBe(false)
    })

    it('returns true when the viewport is 480px or narrower', () => {
        stubMatchMedia(true)
        const { result } = renderHook(() => usePhoneNarrow())
        expect(result.current).toBe(true)
    })

    it('updates when the media query flips', () => {
        const media = stubMatchMedia(false)
        const { result } = renderHook(() => usePhoneNarrow())
        expect(result.current).toBe(false)

        act(() => {
            media.setMatches(true)
            media.listeners.forEach((listener) => listener({ matches: true }))
        })
        expect(result.current).toBe(true)

        act(() => {
            media.setMatches(false)
            media.listeners.forEach((listener) => listener({ matches: false }))
        })
        expect(result.current).toBe(false)
    })

    it('removes the change listener on unmount', () => {
        const media = stubMatchMedia(false)
        const { unmount } = renderHook(() => usePhoneNarrow())
        unmount()
        expect(media.listeners.size).toBe(0)
    })
})
