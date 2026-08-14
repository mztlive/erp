import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useCustomerReceivablesPreview } from './use-customer-receivables-preview'

vi.mock('next/navigation', () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => new URLSearchParams() as unknown as ReadonlyURLSearchParams),
    usePathname: vi.fn(() => '/finance/customer-accounts'),
    useParams: vi.fn(() => ({})),
}))

import { useSearchParams } from 'next/navigation'
import type { ReadonlyURLSearchParams } from 'next/navigation'

const mockedSearchParams = vi.mocked(useSearchParams)

function setup(args: {
    previewKind?: string | null
    previewId?: string
    focusId?: string
}) {
    const patchUrl = vi.fn()
    const hookArgs = {
        view: 'receivable' as const,
        previewKind: (args.previewKind ?? null) as
            | 'receivable'
            | 'receipt'
            | 'invoice'
            | null,
        previewId: args.previewId,
        focusId: args.focusId,
        patchUrl,
    }
    const rendered = renderHook((props) => useCustomerReceivablesPreview(props), {
        initialProps: hookArgs,
    })
    return { ...rendered, patchUrl, hookArgs }
}

beforeEach(() => {
    vi.clearAllMocks()
    mockedSearchParams.mockReturnValue(new URLSearchParams() as unknown as ReadonlyURLSearchParams)
})

describe('useCustomerReceivablesPreview', () => {
    it('derives the initial preview from previewKind + previewId', () => {
        const { result } = setup({
            previewKind: 'receipt',
            previewId: 'rcp_1',
        })
        expect(result.current.preview).toEqual({
            kind: 'receipt',
            id: 'rcp_1',
        })
    })

    it('derives the initial preview from focusId as a receivable', () => {
        const { result } = setup({ focusId: 'acc_1' })
        expect(result.current.preview).toEqual({
            kind: 'receivable',
            id: 'acc_1',
        })
    })

    it('starts closed when neither preview nor focus params are present', () => {
        const { result } = setup({})
        expect(result.current.preview).toBeNull()
    })

    it('does not re-derive the preview when URL params change afterwards', () => {
        const { result, rerender, hookArgs } = setup({})
        rerender({
            ...hookArgs,
            previewKind: 'receipt',
            previewId: 'rcp_1',
        })
        expect(result.current.preview).toBeNull()
    })

    it('openPreview sets state and pushes preview params, clearing focusId', () => {
        const { result, patchUrl } = setup({ focusId: 'acc_1' })
        act(() => {
            result.current.openPreview({ kind: 'invoice', id: 'inv_2' })
        })
        expect(result.current.preview).toEqual({
            kind: 'invoice',
            id: 'inv_2',
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { previewKind: 'invoice', previewId: 'inv_2', focusId: null },
            { replace: false },
        )
    })

    it('openPreview(null) only clears state without touching the URL', () => {
        const { result, patchUrl } = setup({
            previewKind: 'receipt',
            previewId: 'rcp_1',
        })
        act(() => {
            result.current.openPreview(null)
        })
        expect(result.current.preview).toBeNull()
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it('closePreview clears state and removes preview params', () => {
        const { result, patchUrl } = setup({
            previewKind: 'receipt',
            previewId: 'rcp_1',
        })
        act(() => {
            result.current.closePreview()
        })
        expect(result.current.preview).toBeNull()
        expect(patchUrl).toHaveBeenCalledWith(
            { previewKind: null, previewId: null, focusId: null },
            { replace: false },
        )
    })
})
