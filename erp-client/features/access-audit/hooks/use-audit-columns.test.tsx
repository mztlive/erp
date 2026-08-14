import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

import type { AuditEventRow } from '../types'
import { makeAuditRow, makeColumnsInput } from './test-data'
import { useAuditColumns } from './use-audit-columns'

// RTL 自动清理依赖全局 afterEach；vitest globals 关闭，需手动清理。
afterEach(cleanup)

function renderCell(
    columnId: string,
    row: AuditEventRow,
    input = makeColumnsInput(),
) {
    const { result } = renderHook(() => useAuditColumns(input))
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<AuditEventRow, unknown>
    const cell = column!.cell as
        | ((props: CellContext<AuditEventRow, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

describe('useAuditColumns', () => {
    it('returns the expected column ids and headers in order', () => {
        const { result } = renderHook(() =>
            useAuditColumns(makeColumnsInput()),
        )
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'time',
            'actor',
            'role',
            'action',
            'object',
            'result',
            'fields',
            'trace',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '时间',
            '操作者',
            '责任角色',
            '动作',
            '对象',
            '结果',
            '变更字段',
            '请求追踪号',
            '查看',
        ])
    })

    it('renders actor, role, action, object and trace identifiers', () => {
        const row = makeAuditRow()
        const actor = renderCell('actor', row)
        expect(actor.getByText('王小明')).toBeDefined()
        expect(actor.getByText('u1')).toBeDefined()
        expect(renderCell('role', row).getByText('管理员')).toBeDefined()
        expect(renderCell('action', row).getByText('查询审计')).toBeDefined()
        expect(renderCell('object', row).getByText('审计事件 ae-1')).toBeDefined()
        expect(renderCell('trace', row).getByText('trace-1')).toBeDefined()
    })

    it('renders the result label and changed-field display', () => {
        const row = makeAuditRow()
        expect(renderCell('result', row).getByText('成功')).toBeDefined()
        expect(
            renderCell('fields', row).getByText('salary · 已变更'),
        ).toBeDefined()
        expect(
            renderCell('fields', makeAuditRow({ changedFieldDisplay: '—' }))
                .getByText('—'),
        ).toBeDefined()
    })

    it('opens the event detail for the row event id', () => {
        const input = makeColumnsInput()
        const actions = renderCell('actions', makeAuditRow(), input)

        fireEvent.click(actions.getByText('详情'))
        expect(input.openEvent).toHaveBeenCalledWith('ae-1')
    })
})
