import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { FieldPolicyRow } from '../types'
import {
    makeColumnsInput,
    makeFieldRow,
    makeGovernancePolicies,
    makeListView,
} from './test-data'
import { useFieldColumns } from './use-field-columns'

// RTL 自动清理依赖全局 afterEach；vitest globals 关闭，需手动清理。
afterEach(cleanup)

function renderCell(
    columnId: string,
    row: FieldPolicyRow,
    input = makeColumnsInput(),
) {
    const { result } = renderHook(() => useFieldColumns(input))
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<FieldPolicyRow, unknown>
    const cell = column!.cell as
        | ((props: CellContext<FieldPolicyRow, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

describe('useFieldColumns', () => {
    it('returns the expected column ids and headers in order', () => {
        const { result } = renderHook(() =>
            useFieldColumns(makeColumnsInput()),
        )
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'target',
            'subject',
            'caps',
            'mode',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '策略目标',
            '适用',
            '访问能力',
            '可编辑',
            '操作',
        ])
    })

    it('masks the capability cell when the list is field-masked', () => {
        const maskedInput = makeColumnsInput({
            data: makeListView({ emptyReason: 'FIELD_MASKED' }),
        })
        const caps = renderCell(
            'caps',
            makeFieldRow(),
            maskedInput,
        )
        expect(caps.getByText('****')).toBeDefined()

        const visible = renderCell('caps', makeFieldRow())
        expect(visible.getByText('打码 · 可见')).toBeDefined()
    })

    it('renders read-only state for non-editable rows', () => {
        const mode = renderCell('mode', makeFieldRow({ editable: false }))
        expect(mode.getByText('只读')).toBeDefined()

        const actions = renderCell('actions', makeFieldRow({ editable: false }))
        expect(actions.getByText('策略缺失时只读')).toBeDefined()
    })

    it('starts an update-field-policy change when granularity is configured', () => {
        const input = makeColumnsInput({
            policies: {
                ...makeGovernancePolicies(),
                fieldPolicyGranularity: {
                    state: 'CONFIGURED',
                    policyVersion: 'gpv-1',
                    editableTargets: [{ policyTargetId: 'salary', label: '薪资字段' }],
                },
            },
        })
        const actions = renderCell('actions', makeFieldRow(), input)

        fireEvent.click(actions.getByText('调整能力'))

        expect(input.startChange).toHaveBeenCalledTimes(1)
        expect(vi.mocked(input.startChange).mock.calls[0][0]).toMatchObject({
            subjectType: 'FIELD_POLICY',
            subjectId: 'fp-1',
            action: 'UPDATE_FIELD_POLICY',
            granularityPolicyVersion: 'gpv-1',
            policyTargetId: 'salary',
            expectedPermissionVersion: 'pv-live',
        })
    })

    it('ignores the adjust click while the granularity policy is missing', () => {
        const input = makeColumnsInput()
        const actions = renderCell('actions', makeFieldRow(), input)

        fireEvent.click(actions.getByText('调整能力'))
        expect(input.startChange).not.toHaveBeenCalled()
    })
})
