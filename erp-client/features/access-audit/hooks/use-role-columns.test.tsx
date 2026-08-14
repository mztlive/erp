import { cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { RoleRow } from '../types'
import { makeColumnsInput, makeRoleRow } from './test-data'
import { useRoleColumns } from './use-role-columns'

// RTL 自动清理依赖全局 afterEach；vitest globals 关闭，需手动清理弹层。
afterEach(cleanup)

function renderCell(
    columnId: string,
    row: RoleRow,
    input = makeColumnsInput(),
) {
    const { result } = renderHook(() => useRoleColumns(input))
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<RoleRow, unknown>
    const cell = column!.cell as
        | ((props: CellContext<RoleRow, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

async function openActionsMenu(row: RoleRow) {
    fireEvent.mouseDown(screen.getByLabelText(`${row.name} 更多操作`))
    await screen.findByText('删除')
}

describe('useRoleColumns', () => {
    it('returns the expected column ids and headers in order', () => {
        const { result } = renderHook(() =>
            useRoleColumns(makeColumnsInput()),
        )
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'identity',
            'org',
            'perms',
            'scope',
            'status',
            'version',
            'risk',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '角色',
            '组织',
            '模块与动作权限',
            '数据范围',
            '状态',
            '版本',
            '风险',
            '操作',
        ])
    })

    it('renders name, code, permission summary and status label', () => {
        const row = makeRoleRow()
        const identity = renderCell('identity', row)
        expect(identity.getByText('管理员')).toBeDefined()
        expect(identity.getByText('role_code_1')).toBeDefined()

        expect(
            renderCell('perms', row).getByText('查看审计 · 修改角色'),
        ).toBeDefined()
        expect(renderCell('status', row).getByText('启用')).toBeDefined()
        expect(renderCell('version', row).getByText('vlive')).toBeDefined()
    })

    it('renders risk badges for flagged roles and a dash otherwise', () => {
        const flagged = renderCell(
            'risk',
            makeRoleRow({ riskFlags: ['HIGH_PRIVILEGE', 'EMPTY_SCOPE'] }),
        )
        expect(flagged.getByText('高权限')).toBeDefined()
        expect(flagged.getByText('空数据范围')).toBeDefined()

        expect(renderCell('risk', makeRoleRow()).getByText('—')).toBeDefined()
    })

    it('opens the effective-access sheet and routes to edit from the actions cell', () => {
        const input = makeColumnsInput()
        const row = makeRoleRow()
        const actions = renderCell('actions', row, input)

        fireEvent.click(actions.getByText('有效权限'))
        expect(input.openExplain).toHaveBeenCalledWith('ROLE', 'role-1')

        fireEvent.click(actions.getByText('编辑'))
        expect(input.router.push).toHaveBeenCalledWith(
            '/system/roles/role-1/edit',
        )
        expect(actions.getByLabelText('管理员 更多操作')).toBeDefined()
    })

    it('starts an adjust-permissions change from the dropdown menu', async () => {
        const input = makeColumnsInput()
        const row = makeRoleRow()
        renderCell('actions', row, input)

        await openActionsMenu(row)
        fireEvent.click(screen.getByText('调整权限'))

        expect(input.startChange).toHaveBeenCalledTimes(1)
        expect(vi.mocked(input.startChange).mock.calls[0][0]).toMatchObject({
            subjectType: 'ROLE',
            subjectId: 'role-1',
            action: 'UPDATE_ROLE_PERMISSIONS',
            expectedPermissionVersion: 'pv-live',
        })
    })

    it('marks deletion from the dropdown menu with the role identity', async () => {
        const input = makeColumnsInput()
        const row = makeRoleRow()
        renderCell('actions', row, input)

        await openActionsMenu(row)
        fireEvent.click(screen.getByText('删除'))

        expect(input.setDeletingRole).toHaveBeenCalledWith({
            id: 'role-1',
            name: '管理员',
        })
    })

    it('keeps the risk-flagged role from showing the adjust entry', async () => {
        const input = makeColumnsInput()
        renderCell(
            'actions',
            makeRoleRow({ riskFlags: ['HIGH_PRIVILEGE'] }),
            input,
        )

        await openActionsMenu(makeRoleRow())
        expect(screen.queryByText('调整权限')).toBeNull()
        expect(screen.getByText('扩权（将阻断）')).toBeDefined()
    })
})
