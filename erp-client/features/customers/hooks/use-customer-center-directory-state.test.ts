import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useAccountProfileQuery } from '@/features/auth/queries'
import {
    useCustomerCenterDirectoryState,
    useCustomerCenterScopeGuard,
    useCustomerCenterSearchShortcut,
} from '@/features/customers/hooks/use-customer-center-directory-state'
import { writeDirectoryUrl } from '@/features/customers/lib/directory-url'

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock('next/navigation', () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => '/test',
    useParams: () => ({}),
}))

vi.mock('@/features/auth/queries', () => ({
    useAccountProfileQuery: vi.fn(),
}))

const mockedAccountProfile = vi.mocked(useAccountProfileQuery)

function profileState(permissions: string[], isPending = false) {
    return {
        isPending,
        data: { permissions },
    } as unknown as ReturnType<typeof useAccountProfileQuery>
}

describe('useCustomerCenterDirectoryState', () => {
    beforeEach(() => {
        navMocks.searchParams = new URLSearchParams()
        navMocks.replace.mockClear()
        navMocks.push.mockClear()
    })

    it('parses an empty URL into defaults', () => {
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.scope).toBe('mine')
        expect(result.current.status).toBe('active')
        expect(result.current.q).toBe('')
        expect(result.current.sort).toBe('business')
        expect(result.current.dir).toBe('desc')
        expect(result.current.page).toBe(1)
        expect(result.current.searchDraft).toBe('')
        expect(result.current.statusDraft).toBe('active')
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.appliedChips).toEqual([])
        expect(result.current.sorting).toEqual([
            { id: 'business', desc: true },
        ])
    })

    it('parses scope/status/q/dir/page from the URL', () => {
        navMocks.searchParams = new URLSearchParams(
            'scope=collaborating&status=all&q=%E7%94%B2&dir=asc&page=3',
        )
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.scope).toBe('collaborating')
        expect(result.current.status).toBe('all')
        expect(result.current.q).toBe('甲')
        expect(result.current.dir).toBe('asc')
        expect(result.current.page).toBe(3)
        expect(result.current.searchDraft).toBe('甲')
        expect(result.current.statusDraft).toBe('all')
        expect(result.current.hasActiveFilters).toBe(true)
        expect(result.current.sorting).toEqual([
            { id: 'business', desc: false },
        ])
    })

    it('falls back on invalid scope and page values', () => {
        navMocks.searchParams = new URLSearchParams(
            'scope=unknown&status=unknown&page=0&dir=sideways',
        )
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.scope).toBe('mine')
        expect(result.current.status).toBe('active')
        expect(result.current.statusDraft).toBe('active')
        expect(result.current.page).toBe(1)
        expect(result.current.dir).toBe('desc')
    })

    it('writes filter changes to the URL via router.replace', () => {
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        act(() => {
            result.current.pushState({ scope: 'all_authorized', page: 1 })
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?scope=all_authorized',
            { scroll: false },
        )

        act(() => {
            result.current.pushState({ status: 'all', page: 1 })
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?status=all',
            { scroll: false },
        )

        act(() => {
            result.current.pushState({ page: 2 })
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith('/test?page=2', {
            scroll: false,
        })

        act(() => {
            result.current.pushState({ q: '客户甲', page: 1 })
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            writeDirectoryUrl('/test', {
                scope: 'mine',
                status: 'active',
                q: '客户甲',
                sort: 'business',
                dir: 'desc',
                page: 1,
            }),
            { scroll: false },
        )
    })

    it('applies keyword and status drafts in one URL patch and closes the panel', () => {
        navMocks.searchParams = new URLSearchParams('status=all')
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.panelOpen).toBe(true)
        act(() => {
            result.current.setSearchDraft('  客户甲  ')
            result.current.setStatusDraft('disabled')
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?status=disabled&q=%E5%AE%A2%E6%88%B7%E7%94%B2',
            { scroll: false },
        )
        expect(result.current.panelOpen).toBe(false)
    })

    it('applies a scope shortcut directly without touching drafts', () => {
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        act(() => {
            result.current.setSearchDraft('未提交')
            result.current.setStatusDraft('disabled')
        })
        act(() => {
            result.current.applyScope('collaborating')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?scope=collaborating',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('未提交')
        expect(result.current.statusDraft).toBe('disabled')
    })

    it('opens the panel initially when the URL carries structured filters', () => {
        navMocks.searchParams = new URLSearchParams('status=disabled')
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.panelOpen).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
    })

    it('removes a single applied condition', () => {
        navMocks.searchParams = new URLSearchParams(
            'status=disabled&q=%E7%94%B2',
        )
        const { result, rerender } = renderHook(() =>
            useCustomerCenterDirectoryState(),
        )

        act(() => {
            result.current.removeFilter('status')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?q=%E7%94%B2',
            { scroll: false },
        )
        expect(result.current.statusDraft).toBe('active')

        // router.replace 后 useSearchParams 已同步，模拟下一次移除基于最新 URL
        navMocks.replace.mockClear()
        navMocks.searchParams = new URLSearchParams('q=%E7%94%B2')
        rerender()
        act(() => {
            result.current.removeFilter('q')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith('/test', {
            scroll: false,
        })
        expect(result.current.searchDraft).toBe('')
    })

    it('resets only the structured status and keeps the panel open', () => {
        navMocks.searchParams = new URLSearchParams(
            'scope=all_authorized&status=all&q=%E7%94%B2',
        )
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        act(() => {
            result.current.resetMoreFilters()
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?scope=all_authorized&q=%E7%94%B2',
            { scroll: false },
        )
        expect(result.current.statusDraft).toBe('active')
        expect(result.current.panelOpen).toBe(true)
        expect(result.current.searchDraft).toBe('甲')
        expect(result.current.scope).toBe('all_authorized')
    })

    it('maps table pagination state to the next URL page', () => {
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 2,
                pageSize: 20,
            })
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith('/test?page=3', {
            scroll: false,
        })
    })

    it('maps sorting changes to URL and resets to page 1', () => {
        navMocks.searchParams = new URLSearchParams('page=5')
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        act(() => {
            result.current.handleSortingChange([
                { id: 'business', desc: false },
            ])
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith('/test?dir=asc', {
            scroll: false,
        })

        navMocks.replace.mockClear()
        act(() => {
            result.current.handleSortingChange([
                { id: 'unknown-column', desc: false },
            ])
        })
        expect(navMocks.replace).not.toHaveBeenCalled()

        act(() => {
            result.current.handleSortingChange([])
        })
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('clears q/status/page but keeps scope, sort and dir', () => {
        navMocks.searchParams = new URLSearchParams(
            'scope=all_authorized&status=all&q=%E7%94%B2&page=2',
        )
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.panelOpen).toBe(true)
        act(() => {
            result.current.clearAllFilters()
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/test?scope=all_authorized',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('')
        expect(result.current.statusDraft).toBe('active')
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it('derives applied chips from the URL only', () => {
        navMocks.searchParams = new URLSearchParams(
            'status=disabled&q=%E7%94%B2',
        )
        const { result } = renderHook(() => useCustomerCenterDirectoryState())

        expect(result.current.appliedChips).toEqual([
            { key: 'q', label: '搜索：甲' },
            { key: 'status', label: '状态：停用' },
        ])

        navMocks.searchParams = new URLSearchParams('status=all')
        const { result: allStatus } = renderHook(() =>
            useCustomerCenterDirectoryState(),
        )
        expect(allStatus.current.appliedChips).toEqual([
            { key: 'status', label: '状态：全部' },
        ])
    })

    it('syncs the search and status drafts when the URL changes', () => {
        const { result, rerender } = renderHook(() =>
            useCustomerCenterDirectoryState(),
        )

        navMocks.searchParams = new URLSearchParams('q=%E4%B9%99')
        rerender()
        expect(result.current.searchDraft).toBe('乙')

        navMocks.searchParams = new URLSearchParams('status=disabled')
        rerender()
        expect(result.current.statusDraft).toBe('disabled')
    })
})

describe('useCustomerCenterScopeGuard', () => {
    const baseState = {
        scope: 'all_authorized' as const,
        status: 'active' as const,
        q: '',
        sort: 'business',
        dir: 'desc' as const,
        page: 2,
    }

    beforeEach(() => {
        navMocks.replace.mockClear()
        mockedAccountProfile.mockReset()
    })

    it('derives canCreate and canReadAll from the account permissions', () => {
        mockedAccountProfile.mockReturnValue(
            profileState(['customer:create', 'customer_scope:detail']),
        )
        const { result } = renderHook(() =>
            useCustomerCenterScopeGuard(baseState),
        )

        expect(result.current.canCreate).toBe(true)
        expect(result.current.canReadAll).toBe(true)
    })

    it('supports wildcard permissions', () => {
        mockedAccountProfile.mockReturnValue(profileState(['*:*']))
        const { result } = renderHook(() =>
            useCustomerCenterScopeGuard(baseState),
        )

        expect(result.current.canCreate).toBe(true)
        expect(result.current.canReadAll).toBe(true)
    })

    it('redirects all_authorized back to mine when the scope is not granted', () => {
        mockedAccountProfile.mockReturnValue(
            profileState(['customer:create']),
        )
        renderHook(() => useCustomerCenterScopeGuard(baseState))

        expect(navMocks.replace).toHaveBeenCalledWith('/test', {
            scroll: false,
        })
    })

    it('does not redirect when the scope is granted', () => {
        mockedAccountProfile.mockReturnValue(
            profileState(['customer_scope:detail']),
        )
        renderHook(() => useCustomerCenterScopeGuard(baseState))

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('does not redirect outside all_authorized', () => {
        mockedAccountProfile.mockReturnValue(profileState([]))
        renderHook(() =>
            useCustomerCenterScopeGuard({ ...baseState, scope: 'mine' }),
        )

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('waits for the account profile before redirecting', () => {
        mockedAccountProfile.mockReturnValue(profileState([], true))
        renderHook(() => useCustomerCenterScopeGuard(baseState))

        expect(navMocks.replace).not.toHaveBeenCalled()
    })
})

describe('useCustomerCenterSearchShortcut', () => {
    let unmountHook: (() => void) | undefined

    afterEach(() => {
        unmountHook?.()
        unmountHook = undefined
        document
            .querySelectorAll('[data-slot="customer-search"]')
            .forEach((node) => node.remove())
        document
            .querySelectorAll('[role="dialog"]')
            .forEach((node) => node.remove())
    })

    function mountShortcut() {
        const { unmount } = renderHook(() => useCustomerCenterSearchShortcut())
        unmountHook = unmount
        return unmount
    }

    function keydown(init: KeyboardEventInit = {}) {
        return new KeyboardEvent('keydown', {
            key: '/',
            cancelable: true,
            ...init,
        })
    }

    it('focuses the customer search input when "/" is pressed outside inputs', () => {
        const input = document.createElement('input')
        input.setAttribute('data-slot', 'customer-search')
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')

        mountShortcut()

        const event = keydown()
        window.dispatchEvent(event)
        expect(focusSpy).toHaveBeenCalledTimes(1)
        expect(event.defaultPrevented).toBe(true)
    })

    it('ignores the shortcut while typing in inputs', () => {
        const input = document.createElement('input')
        input.setAttribute('data-slot', 'customer-search')
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')

        mountShortcut()

        const source = document.createElement('input')
        document.body.appendChild(source)
        const event = keydown({ bubbles: true })
        source.dispatchEvent(event)
        expect(focusSpy).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)

        source.remove()
    })

    it('ignores the shortcut when meta/ctrl is held', () => {
        const input = document.createElement('input')
        input.setAttribute('data-slot', 'customer-search')
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')

        mountShortcut()

        const event = keydown({ metaKey: true })
        window.dispatchEvent(event)
        expect(focusSpy).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)
    })

    it('ignores the shortcut when a dialog is open', () => {
        const input = document.createElement('input')
        input.setAttribute('data-slot', 'customer-search')
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')
        const dialog = document.createElement('div')
        dialog.setAttribute('role', 'dialog')
        document.body.appendChild(dialog)

        mountShortcut()

        const event = keydown()
        window.dispatchEvent(event)
        expect(focusSpy).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)
    })

    it('removes the listener on unmount', () => {
        const input = document.createElement('input')
        input.setAttribute('data-slot', 'customer-search')
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')

        const unmount = mountShortcut()
        unmount()
        unmountHook = undefined

        window.dispatchEvent(keydown())
        expect(focusSpy).not.toHaveBeenCalled()
    })
})
