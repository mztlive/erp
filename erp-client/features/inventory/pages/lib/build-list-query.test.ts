import { describe, expect, it } from 'vitest'

import { buildListQuery } from './build-list-query'
import type { BuildListQueryInput } from './build-list-query'

const base: BuildListQueryInput = {
    view: 'balance',
    qParam: '',
    warehouseId: undefined,
    skuId: undefined,
    salesOrderLineId: undefined,
    availability: 'all',
    movementType: [],
    occurredFrom: undefined,
    occurredTo: undefined,
    cursorParam: undefined,
    pageSize: 20,
    sortValue: 'warehouseCode:asc,skuCode:asc',
    balanceIdParam: undefined,
    adjustmentIdParam: undefined,
}

describe('buildListQuery', () => {
    it('builds a default query from empty params', () => {
        expect(buildListQuery(base)).toEqual({
            view: 'balance',
            q: undefined,
            warehouseId: undefined,
            skuId: undefined,
            salesOrderLineId: undefined,
            availability: 'all',
            movementType: [],
            occurredFrom: undefined,
            occurredTo: undefined,
            cursor: undefined,
            pageSize: 20,
            sort: ['warehouseCode:asc', 'skuCode:asc'],
            balanceId: undefined,
            adjustmentId: undefined,
        })
    })

    it('maps every param onto the query', () => {
        expect(
            buildListQuery({
                ...base,
                view: 'movement',
                qParam: 'SKU-1',
                warehouseId: 'w1',
                skuId: 's1',
                salesOrderLineId: 'l1',
                availability: 'zero',
                movementType: ['PURCHASE_RECEIPT'],
                occurredFrom: '2026-08-01',
                occurredTo: '2026-08-14',
                cursorParam: 'w10:movement:40',
                pageSize: 50,
                sortValue: 'occurredAt:desc,movementId:desc',
                balanceIdParam: 'b1',
                adjustmentIdParam: 'a1',
            }),
        ).toEqual({
            view: 'movement',
            q: 'SKU-1',
            warehouseId: 'w1',
            skuId: 's1',
            salesOrderLineId: 'l1',
            availability: 'zero',
            movementType: ['PURCHASE_RECEIPT'],
            occurredFrom: '2026-08-01',
            occurredTo: '2026-08-14',
            cursor: 'w10:movement:40',
            pageSize: 50,
            sort: ['occurredAt:desc', 'movementId:desc'],
            balanceId: 'b1',
            adjustmentId: 'a1',
        })
    })

    it('coalesces an empty q to undefined', () => {
        expect(buildListQuery({ ...base, qParam: '  ' }).q).toBe('  ')
        expect(buildListQuery({ ...base, qParam: '' }).q).toBeUndefined()
    })

    it('splits the sort value and drops empty tokens', () => {
        expect(
            buildListQuery({ ...base, sortValue: 'a:asc,,b:desc,' }).sort,
        ).toEqual(['a:asc', 'b:desc'])
    })
})
