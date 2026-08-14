import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useObjectCenterSection } from './consumption-order-center-hooks'

vi.mock('next/navigation', () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => new URLSearchParams() as unknown as ReadonlyURLSearchParams),
    usePathname: vi.fn(() => '/commerce/consumption-orders/m-1'),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from 'next/navigation'
import type { ReadonlyURLSearchParams } from 'next/navigation'

const mockedRouter = vi.mocked(useRouter)
const mockedSearchParams = vi.mocked(useSearchParams)
const mockedPathname = vi.mocked(usePathname)

type RouterLike = ReturnType<typeof useRouter>

let router: {
    push: ReturnType<typeof vi.fn>
    replace: ReturnType<typeof vi.fn>
    back: ReturnType<typeof vi.fn>
}

function setupRouter() {
    router = { push: vi.fn(), replace: vi.fn(), back: vi.fn() }
    mockedRouter.mockReturnValue(router as unknown as RouterLike)
    return router
}

beforeEach(() => {
    vi.clearAllMocks()
    mockedSearchParams.mockReturnValue(new URLSearchParams() as unknown as ReadonlyURLSearchParams)
    mockedPathname.mockReturnValue('/commerce/consumption-orders/m-1')
    setupRouter()
})

describe('useObjectCenterSection', () => {
    it('applies defaults when no params are present', () => {
        const { result } = renderHook(() => useObjectCenterSection())
        expect(result.current.section).toBe('overview')
        expect(result.current.factId).toBeUndefined()
        expect(result.current.backToListHref).toBe(
            '/commerce/consumption-orders',
        )
    })

    it('parses section and fact from the URL', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('section=facts&fact=f-1') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        expect(result.current.section).toBe('facts')
        expect(result.current.factId).toBe('f-1')
    })

    it('falls back to overview for unknown section values', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('section=unknown') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        expect(result.current.section).toBe('overview')
    })

    it('uses the returnTo param for the back link', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'returnTo=%2Fcommerce%2Fconsumption-orders%3Fpage%3D2',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        expect(result.current.backToListHref).toBe(
            '/commerce/consumption-orders?page=2',
        )
    })

    it('setSection replaces the URL with the new section', () => {
        const { result } = renderHook(() => useObjectCenterSection())
        act(() => {
            result.current.setSection('supplier')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders/m-1?section=supplier',
        )
    })

    it('setSection keeps other params and appends the section', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('returnTo=%2Fcommerce%2Fconsumption-orders') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        act(() => {
            result.current.setSection('cost')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders/m-1?returnTo=%2Fcommerce%2Fconsumption-orders&section=cost',
        )
    })

    it('setSection sets the fact param alongside the facts section', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('section=overview') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        act(() => {
            result.current.setSection('facts', 'f-2')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders/m-1?section=facts&fact=f-2',
        )
    })

    it('setSection drops the fact param when leaving facts', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('section=facts&fact=f-1') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        act(() => {
            result.current.setSection('overview')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders/m-1?section=overview',
        )
    })

    it('setSection keeps the existing fact when re-selecting facts', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('section=facts&fact=f-1') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useObjectCenterSection())
        act(() => {
            result.current.setSection('facts')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders/m-1?section=facts&fact=f-1',
        )
    })
})
