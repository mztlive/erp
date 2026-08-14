import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useSalesOrderDetailUrlState } from '@/features/sales-orders/hooks/use-sales-order-detail-url-state'

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock('next/navigation', () => ({
    useRouter: () => ({
        push: vi.fn(),
        replace: navMocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => '/test',
    useParams: () => ({}),
}))

describe('useSalesOrderDetailUrlState', () => {
    beforeEach(() => {
        navMocks.searchParams = new URLSearchParams()
        navMocks.replace.mockClear()
    })

    it('parses an empty URL into defaults', () => {
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        expect(result.current.returnTo).toBeNull()
        expect(result.current.fromWorkspace).toBeNull()
        expect(result.current.pageMode).toBeNull()
        expect(result.current.focusedWorkItemId).toBe('')
        expect(result.current.queueContextId).toBe('')
        expect(result.current.workItemReturnTo).toBe('/workspace/tasks')
        expect(result.current.fromQueue).toBe(false)
        expect(result.current.backHref).toBe('/sales/orders')
        expect(result.current.backLabel).toBe('返回列表')
    })

    it('parses and trims workItemId/queueContextId from the URL', () => {
        navMocks.searchParams = new URLSearchParams(
            'returnTo=%2Fworkspace%2Ftasks&from=W07&mode=edit&workItemId=%20wi_1%20&queueContextId=qc_1',
        )
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        expect(result.current.returnTo).toBe('/workspace/tasks')
        expect(result.current.fromWorkspace).toBe('W07')
        expect(result.current.pageMode).toBe('edit')
        expect(result.current.focusedWorkItemId).toBe('wi_1')
        expect(result.current.queueContextId).toBe('qc_1')
        expect(result.current.fromQueue).toBe(true)
        expect(result.current.backHref).toBe('/workspace/tasks')
        expect(result.current.backLabel).toBe('返回采购确认')
    })

    it('derives workItemReturnTo from queueContextId + workItemId', () => {
        navMocks.searchParams = new URLSearchParams(
            'queueContextId=qc_9&workItemId=wi_9',
        )
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        expect(result.current.workItemReturnTo).toBe(
            '/workspace/tasks?queueContextId=qc_9&currentWorkItemId=wi_9',
        )
    })

    it('returns queue back labels per workspace', () => {
        for (const [from, label] of [
            ['W07', '返回采购确认'],
            ['W08', '返回采购单列表'],
            ['W09', '返回履约处理'],
            ['W11', '返回列表'],
        ] as const) {
            navMocks.searchParams = new URLSearchParams(
                `returnTo=%2Fworkspace%2Ftasks&from=${from}`,
            )
            const { result } = renderHook(() =>
                useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
            )
            expect(result.current.backLabel).toBe(label)
        }
    })

    it('replaceOrderHref preserves existing params and sets section/mode', () => {
        navMocks.searchParams = new URLSearchParams(
            'returnTo=%2Fqueue&from=W09',
        )
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.replaceOrderHref({
                section: 'versions',
                mode: 'edit',
            })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?returnTo=%2Fqueue&from=W09&section=versions&mode=edit',
            { scroll: false },
        )
    })

    it('replaceOrderHref deletes mode when patch.mode is null', () => {
        navMocks.searchParams = new URLSearchParams('mode=edit')
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.replaceOrderHref({
                section: 'procurement-rejection',
                mode: null,
            })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?section=procurement-rejection',
            { scroll: false },
        )
    })

    it('replaceOrderHref drops the query entirely when nothing remains', () => {
        navMocks.searchParams = new URLSearchParams('mode=edit')
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.replaceOrderHref({ mode: null })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1',
            { scroll: false },
        )
    })

    it('selectSection carries returnTo/from and keeps workItemId only for work sections', () => {
        navMocks.searchParams = new URLSearchParams(
            'returnTo=%2Fqueue&from=W07&workItemId=wi_1&queueContextId=qc_1',
        )
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.selectSection('approval')
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?section=approval&returnTo=%2Fqueue&from=W07&workItemId=wi_1&queueContextId=qc_1',
            { scroll: false },
        )
    })

    it('selectSection drops workItemId for regular sections but keeps queueContextId when both exist', () => {
        navMocks.searchParams = new URLSearchParams(
            'returnTo=%2Fqueue&workItemId=wi_1&queueContextId=qc_1',
        )
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.selectSection('fulfillment')
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?section=fulfillment&returnTo=%2Fqueue&queueContextId=qc_1',
            { scroll: false },
        )
    })

    it('selectSection drops queueContextId when no workItemId is present', () => {
        navMocks.searchParams = new URLSearchParams(
            'returnTo=%2Fqueue&queueContextId=qc_1',
        )
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.selectSection('fulfillment')
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?section=fulfillment&returnTo=%2Fqueue',
            { scroll: false },
        )
    })

    it('enterRejectionEdit opens the rejection editor with mode=edit', () => {
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.enterRejectionEdit()
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?section=procurement-rejection&mode=edit',
            { scroll: false },
        )
    })

    it('leaveRejectionEdit clears mode but keeps the section', () => {
        navMocks.searchParams = new URLSearchParams('mode=edit')
        const { result } = renderHook(() =>
            useSalesOrderDetailUrlState({ salesOrderId: 'SO-1' }),
        )

        act(() => {
            result.current.leaveRejectionEdit()
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/orders/SO-1?section=procurement-rejection',
            { scroll: false },
        )
    })
})
