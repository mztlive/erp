import { describe, it, expect } from 'vitest'

import { describeCustomerDirectoryTable } from '@/features/customers/lib/customer-center-description'

const base = {
    scope: 'mine' as const,
    status: 'active' as const,
    q: '',
    totalInScope: 5,
    itemsLength: 5,
}

describe('describeCustomerDirectoryTable', () => {
    it('describes an empty scope without filters', () => {
        expect(
            describeCustomerDirectoryTable({
                ...base,
                scope: 'collaborating',
                totalInScope: 0,
                itemsLength: 0,
            }),
        ).toBe('协作客户下还没有客户。有权时可新建客户。')
    })

    it('describes a filter miss with status and keyword suffixes', () => {
        expect(
            describeCustomerDirectoryTable({
                ...base,
                scope: 'all_authorized',
                status: 'disabled',
                q: '甲',
                totalInScope: 12,
                itemsLength: 0,
            }),
        ).toBe('当前筛选无结果：全部有权客户 · 停用 · “甲”')
    })

    it('describes an active filter hit', () => {
        expect(
            describeCustomerDirectoryTable({
                ...base,
                status: 'all',
                totalInScope: 3,
                itemsLength: 3,
            }),
        ).toBe('当前筛选：我的客户 · 全部状态')
    })

    it('describes the unfiltered default view', () => {
        expect(describeCustomerDirectoryTable(base)).toBe(
            '我的客户下的全部客户；本页用于选择客户并进入其详情。',
        )
    })

    it('keeps keyword rendering empty when q is only whitespace', () => {
        expect(
            describeCustomerDirectoryTable({
                ...base,
                q: '   ',
                totalInScope: 2,
                itemsLength: 2,
            }),
        ).toBe('我的客户下的全部客户；本页用于选择客户并进入其详情。')
    })
})
