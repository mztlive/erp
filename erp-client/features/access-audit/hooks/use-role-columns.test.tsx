import { cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

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
            'perms',
            'accounts',
            'scope',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '角色',
            '权限覆盖',
            '绑定账号',
            '数据范围',
            '操作',
        ])
    })

    it('renders name, code and a per-module permission summary', () => {
        const row = makeRoleRow()
        const identity = renderCell('identity', row)
        expect(identity.getByText('管理员')).toBeDefined()
        expect(identity.getByText('role_code_1')).toBeDefined()

        const perms = renderCell('perms', row)
        expect(perms.getByText('12')).toBeDefined()
        expect(perms.getByText(/系统审计/)).toBeDefined()
        expect(perms.getByText(/角色管理/)).toBeDefined()
    })

    it('calls out wildcard and empty permission sets instead of printing codes', () => {
        expect(
            renderCell(
                'perms',
                makeRoleRow({ allPermissions: true, permissionCount: 0 }),
            ).getByText('全部权限'),
        ).toBeDefined()

        expect(
            renderCell(
                'perms',
                makeRoleRow({ permissionCount: 0, permissionGroups: [] }),
            ).getByText('无权限条目'),
        ).toBeDefined()
    })

    it('shows how many modules are hidden beyond the first three', () => {
        const perms = renderCell(
            'perms',
            makeRoleRow({
                permissionGroups: [
                    { name: '客户', count: 4 },
                    { name: '销售单', count: 3 },
                    { name: '合同', count: 2 },
                    { name: '库存', count: 1 },
                    { name: '采购单', count: 1 },
                ],
            }),
        )
        expect(perms.getByText('+2 个模块')).toBeDefined()
    })

    it('renders bound account count and a dash when nothing is bound', () => {
        expect(renderCell('accounts', makeRoleRow()).getByText('3')).toBeDefined()
        expect(
            renderCell(
                'accounts',
                makeRoleRow({ boundAccountCount: 0 }),
            ).getByText('—'),
        ).toBeDefined()
    })

    it('routes to the role form from the actions cell', () => {
        const input = makeColumnsInput()
        const row = makeRoleRow()
        const actions = renderCell('actions', row, input)

        fireEvent.click(actions.getByText('编辑'))
        expect(input.router.push).toHaveBeenCalledWith(
            '/system/roles/role-1/edit',
        )
        expect(actions.getByLabelText('管理员 更多操作')).toBeDefined()
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

    it('does not offer change commands that the backend always blocks', async () => {
        const input = makeColumnsInput()
        renderCell('actions', makeRoleRow(), input)

        await openActionsMenu(makeRoleRow())
        expect(screen.queryByText('调整权限')).toBeNull()
        expect(screen.queryByText('扩权（将阻断）')).toBeNull()
        expect(screen.queryByText('停用')).toBeNull()
        expect(input.startChange).not.toHaveBeenCalled()
    })
})
