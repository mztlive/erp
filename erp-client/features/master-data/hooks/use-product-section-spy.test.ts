import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { PRODUCT_EDITOR_SECTIONS } from '@/features/master-data/lib/product-editor-model'
import { useProductSectionSpy } from './use-product-section-spy'

type FakeEntry = { target: Element; isIntersecting: boolean }

class FakeIntersectionObserver {
    static instances: FakeIntersectionObserver[] = []
    callback: IntersectionObserverCallback
    observed: Element[] = []
    disconnected = false

    constructor(callback: IntersectionObserverCallback) {
        this.callback = callback
        FakeIntersectionObserver.instances.push(this)
    }

    observe(el: Element) {
        this.observed.push(el)
    }

    unobserve() {}

    disconnect() {
        this.disconnected = true
    }

    takeRecords(): IntersectionObserverEntry[] {
        return []
    }

    trigger(entry: FakeEntry) {
        this.callback(
            [entry as unknown as IntersectionObserverEntry],
            this as unknown as IntersectionObserver,
        )
    }
}

function renderSections() {
    for (const section of PRODUCT_EDITOR_SECTIONS) {
        const el = document.createElement('section')
        el.id = `product-section-${section.id}`
        document.body.appendChild(el)
    }
}

describe('useProductSectionSpy', () => {
    beforeEach(() => {
        vi.stubGlobal('IntersectionObserver', FakeIntersectionObserver)
        FakeIntersectionObserver.instances = []
    })

    afterEach(() => {
        vi.unstubAllGlobals()
        document.body.innerHTML = ''
    })

    it('starts on the basic section and does not observe while creating', () => {
        const { result } = renderHook(() =>
            useProductSectionSpy(true, undefined),
        )

        expect(result.current.activeSection).toBe('basic')
        expect(FakeIntersectionObserver.instances).toHaveLength(0)

        act(() => result.current.setActiveSection('effective'))
        expect(result.current.activeSection).toBe('effective')
    })

    it('observes every product section once the detail is loaded', () => {
        renderSections()
        const { unmount } = renderHook(() =>
            useProductSectionSpy(false, 'p1'),
        )

        const [observer] = FakeIntersectionObserver.instances
        expect(observer.observed.map((el) => el.id)).toEqual([
            'product-section-basic',
            'product-section-media',
            'product-section-sku',
            'product-section-effective',
            'product-section-history',
        ])

        unmount()
        expect(observer.disconnected).toBe(true)
    })

    it('highlights the section currently intersecting the viewport band', () => {
        renderSections()
        const { result } = renderHook(() =>
            useProductSectionSpy(false, 'p1'),
        )

        const [observer] = FakeIntersectionObserver.instances
        const effective = document.getElementById(
            'product-section-effective',
        )!
        act(() => {
            observer.trigger({ target: effective, isIntersecting: true })
        })
        expect(result.current.activeSection).toBe('effective')

        act(() => {
            observer.trigger({ target: effective, isIntersecting: false })
        })
        expect(result.current.activeSection).toBe('effective')
    })

    it('re-attaches the observer when the product identity changes', () => {
        renderSections()
        const { rerender } = renderHook(
            ({ stableId }: { stableId: string }) =>
                useProductSectionSpy(false, stableId),
            { initialProps: { stableId: 'p1' } },
        )

        expect(FakeIntersectionObserver.instances).toHaveLength(1)
        rerender({ stableId: 'p2' })
        expect(FakeIntersectionObserver.instances).toHaveLength(2)
    })
})
