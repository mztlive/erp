import { cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { UserRow } from '../types'
import { makeColumnsInput, makeUserRow } from './test-data'
import { useUserColumns } from './use-user-columns'

// RTL 自动清理依赖全局 afterEach；vitest globals 关闭，需手动清理弹层。
afterEach(cleanup)

function renderCell(
    columnId: string,
    row: UserRow,
    input = makeColumnsInput(),
) {
    const { result } = renderHook(() => useUserColumns(input))
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<UserRow, unknown>
    const cell = column!.cell as
        | ((props: CellContext<UserRow, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

async function openActionsMenu(row: UserRow) {
    fireEvent.mouseDown(screen.getByLabelText(`${row.displayName} 更多操作`))
    await screen.findByText('紧急撤权')
}

describe('useUserColumns', () => {
    it('returns the expected column ids and headers in order', () => {
        const { result } = renderHook(() =>
            useUserColumns(makeColumnsInput()),
        )
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'identity',
            'roles',
            'scope',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '用户',
            '当前角色',
            '数据范围',
            '操作',
        ])
    })

    it('renders the login account rather than the internal account id', () => {
        const row = makeUserRow()
        const identity = renderCell('identity', row)
        expect(identity.getByText('王小明')).toBeDefined()
        expect(identity.getByText('wangxm')).toBeDefined()
        expect(identity.queryByText('u1')).toBeNull()
        expect(renderCell('roles', row).getByText('管理员')).toBeDefined()
        expect(renderCell('scope', row).getByText('公司级')).toBeDefined()
    })

    it('opens the role assignment dialog from the actions cell', () => {
        const input = makeColumnsInput()
        const row = makeUserRow()
        const actions = renderCell('actions', row, input)

        fireEvent.click(actions.getByText('调整角色'))
        expect(input.setRoleAssignment).toHaveBeenCalledWith({
            userId: 'u1',
            displayName: '王小明',
            accountName: 'wangxm',
            roleIds: ['role-1'],
        })
    })

    it('starts an emergency revoke from the dropdown menu', async () => {
        const input = makeColumnsInput()
        const row = makeUserRow()
        renderCell('actions', row, input)

        await openActionsMenu(row)
        fireEvent.click(screen.getByText('紧急撤权'))

        expect(input.startChange).toHaveBeenCalledTimes(1)
        expect(vi.mocked(input.startChange).mock.calls[0][0]).toMatchObject({
            subjectType: 'USER',
            subjectId: 'u1',
            action: 'EMERGENCY_REVOKE_USER_ROLE',
            roleAssignmentId: 'ura-1',
            expectedPermissionVersion: 'pv-live',
            reasonCode: 'EMERGENCY_STOP_LOSS',
        })
    })

    it('hides the whole menu when the row has no role assignment', () => {
        const input = makeColumnsInput()
        const row = makeUserRow({ roleAssignmentId: undefined })
        const actions = renderCell('actions', row, input)

        expect(actions.queryByLabelText('王小明 更多操作')).toBeNull()
    })

    it('keeps account deletion out of the permission surface', async () => {
        const input = makeColumnsInput()
        const row = makeUserRow()
        renderCell('actions', row, input)

        await openActionsMenu(row)
        expect(screen.queryByText('删除账号')).toBeNull()
    })
})
