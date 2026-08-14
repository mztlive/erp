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
    await screen.findByText('删除')
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
            'period',
            'scope',
            'status',
            'risk',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '用户',
            '当前角色',
            '有效期间',
            '数据范围',
            '账号状态',
            '风险',
            '操作',
        ])
    })

    it('renders display name, account id, roles and status label', () => {
        const row = makeUserRow()
        const identity = renderCell('identity', row)
        expect(identity.getByText('王小明')).toBeDefined()
        expect(identity.getByText('u1')).toBeDefined()
        expect(renderCell('roles', row).getByText('管理员')).toBeDefined()
        expect(renderCell('status', row).getByText('启用')).toBeDefined()
    })

    it('shows 长期 in the period cell when no expiry is recorded', () => {
        const period = renderCell('period', makeUserRow())
        expect(period.container.textContent).toContain('长期')
    })

    it('opens the explain sheet and fills the account form from the actions cell', () => {
        const input = makeColumnsInput()
        const row = makeUserRow()
        const actions = renderCell('actions', row, input)

        fireEvent.click(actions.getByText('有效权限'))
        expect(input.openExplain).toHaveBeenCalledWith('USER', 'u1')

        fireEvent.click(actions.getByText('编辑'))
        expect(input.setAccountForm).toHaveBeenCalledWith({
            mode: 'edit',
            account: {
                id: 'u1',
                account: 'wangxm',
                name: '王小明',
                role_ids: ['role-1'],
            },
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

    it('hides the revoke entry when the row has no role assignment', async () => {
        const input = makeColumnsInput()
        const row = makeUserRow({ roleAssignmentId: undefined })
        renderCell('actions', row, input)

        await openActionsMenu(row)
        expect(screen.queryByText('紧急撤权')).toBeNull()
    })

    it('marks account deletion from the dropdown menu', async () => {
        const input = makeColumnsInput()
        const row = makeUserRow()
        renderCell('actions', row, input)

        await openActionsMenu(row)
        fireEvent.click(screen.getByText('删除'))

        expect(input.setDeletingAccount).toHaveBeenCalledWith({
            id: 'u1',
            account: 'wangxm',
        })
    })
})
