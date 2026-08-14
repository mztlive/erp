import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, render, renderHook } from '@testing-library/react'

import { useProductStickyHeader } from './use-product-sticky-header'

class FakeResizeObserver {
    static instances: FakeResizeObserver[] = []
    callback: ResizeObserverCallback
    observed: Element[] = []

    constructor(callback: ResizeObserverCallback) {
        this.callback = callback
        FakeResizeObserver.instances.push(this)
    }

    observe(el: Element) {
        this.observed.push(el)
    }

    unobserve() {}

    disconnect() {}

    trigger() {
        this.callback([], this as unknown as ResizeObserver)
    }
}

function Harness({ stableId }: { stableId: string }) {
    const { stickyHeaderRef, stickyHeaderHeight, sectionScrollMarginPx } =
        useProductStickyHeader(false, stableId, 3)
    return (
        <div>
            <header
                ref={stickyHeaderRef}
                data-testid="sticky-header"
                style={{ height: 200 }}
            />
            <output data-testid="height">{stickyHeaderHeight}</output>
            <output data-testid="margin">{sectionScrollMarginPx}</output>
        </div>
    )
}

describe('useProductStickyHeader', () => {
    beforeEach(() => {
        vi.stubGlobal('ResizeObserver', FakeResizeObserver)
        FakeResizeObserver.instances = []
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('keeps the default height until the header is mounted', () => {
        const { result } = renderHook(() =>
            useProductStickyHeader(false, 'p1', 3),
        )

        expect(result.current.stickyHeaderHeight).toBe(160)
        expect(result.current.sectionScrollMarginPx).toBe(172)
        expect(FakeResizeObserver.instances).toHaveLength(0)
    })

    it('re-measures the mounted header and derives the scroll margin', () => {
        const { getByTestId } = render(<Harness stableId="p1" />)
        const header = getByTestId('sticky-header')
        header.getBoundingClientRect = () => ({ height: 200 }) as DOMRect

        const [observer] = FakeResizeObserver.instances
        expect(observer.observed[0]).toBe(header)

        act(() => {
            observer.trigger()
        })

        expect(getByTestId('height').textContent).toBe('200')
        expect(getByTestId('margin').textContent).toBe('212')
    })

    it('re-measures when the product identity changes', () => {
        const { rerender } = render(<Harness stableId="p1" />)

        expect(FakeResizeObserver.instances).toHaveLength(1)
        rerender(<Harness stableId="p2" />)
        expect(FakeResizeObserver.instances).toHaveLength(2)
    })
})
