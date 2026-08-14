import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

import type { ScopeRow } from '../types'
import { makeColumnsInput, makeScopeRow } from './test-data'
import { useScopeColumns } from './use-scope-columns'

// RTL 自动清理依赖全局 afterEach；vitest globals 关闭，需手动清理。
afterEach(cleanup)

function renderCell(
    columnId: string,
    row: ScopeRow,
    input = makeColumnsInput(),
) {
    const { result } = renderHook(() => useScopeColumns(input))
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<ScopeRow, unknown>
    const cell = column!.cell as
        | ((props: CellContext<ScopeRow, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

describe('useScopeColumns', () => {
    it('returns the expected column ids and headers in order', () => {
        const { result } = renderHook(() =>
            useScopeColumns(makeColumnsInput()),
        )
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'subject',
            'type',
            'targets',
            'risk',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '主体',
            '范围类型',
            '范围对象',
            '风险',
            '操作',
        ])
    })

    it('labels the subject as role or user by subject type', () => {
        const role = renderCell('subject', makeScopeRow())
        expect(role.getByText('管理员')).toBeDefined()
        expect(role.getByText('角色')).toBeDefined()

        const user = renderCell(
            'subject',
            makeScopeRow({ subjectType: 'USER', subjectId: 'u1' }),
        )
        expect(user.getByText('用户')).toBeDefined()
    })

    it('joins risk labels and falls back to a dash', () => {
        const flagged = renderCell(
            'risk',
            makeScopeRow({ riskFlags: ['HIGH_PRIVILEGE', 'EXPIRING_SOON'] }),
        )
        expect(flagged.container.textContent).toBe('高权限、即将过期')
        expect(
            renderCell('risk', makeScopeRow()).container.textContent,
        ).toBe('—')
    })

    it('opens the explain sheet for the row subject', () => {
        const input = makeColumnsInput()
        const actions = renderCell('actions', makeScopeRow(), input)

        fireEvent.click(actions.getByText('有效权限'))
        expect(input.openExplain).toHaveBeenCalledWith('ROLE', 'role-1')
    })
})
