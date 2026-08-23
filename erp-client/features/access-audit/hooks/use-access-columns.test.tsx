import { renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { makeColumnsInput } from './test-data'
import { useAccessColumns } from './use-access-columns'

describe('useAccessColumns', () => {
    it('composes the three per-view column sets with stable ids', () => {
        const { result } = renderHook(() =>
            useAccessColumns(makeColumnsInput()),
        )

        expect(Object.keys(result.current).sort()).toEqual([
            'auditColumns',
            'roleColumns',
            'userColumns',
        ])
        expect(result.current.roleColumns.map((c) => c.id)).toContain(
            'identity',
        )
        expect(result.current.roleColumns.map((c) => c.id)).toContain(
            'accounts',
        )
        expect(result.current.userColumns.map((c) => c.id)).toContain('roles')
        expect(result.current.auditColumns.map((c) => c.id)).toContain('trace')
    })

    it('memoizes each column set per unchanged input reference', () => {
        const input = makeColumnsInput()
        const { result, rerender } = renderHook(
            ({ current }: { current: ReturnType<typeof makeColumnsInput> }) =>
                useAccessColumns(current),
            { initialProps: { current: input } },
        )
        const first = result.current
        rerender({ current: input })
        expect(result.current.roleColumns).toBe(first.roleColumns)
        expect(result.current.userColumns).toBe(first.userColumns)
        expect(result.current.auditColumns).toBe(first.auditColumns)

        rerender({ current: makeColumnsInput() })
        expect(result.current.roleColumns).not.toBe(first.roleColumns)
        expect(result.current.auditColumns).not.toBe(first.auditColumns)
    })

    it('feeds data and policies through to the dependent column hooks', () => {
        const input = makeColumnsInput({ data: undefined, policies: undefined })
        const { result } = renderHook(() => useAccessColumns(input))
        expect(result.current.roleColumns.length).toBeGreaterThan(0)
        expect(result.current.userColumns.length).toBeGreaterThan(0)
    })
})
