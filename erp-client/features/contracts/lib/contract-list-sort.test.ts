import { describe, it, expect } from 'vitest'
import type { SortingState } from '@tanstack/react-table'

import { sortRows } from '@/features/contracts/lib/contract-list-sort'
import type { ContractListRow } from '@/features/contracts/types'

let seq = 0
function row(overrides: Partial<ContractListRow> = {}): ContractListRow {
    seq += 1
    return {
        contractId: `ct-${seq}`,
        contractNo: `CT-${seq}`,
        customer: {
            customerId: `c-${seq}`,
            customerNo: `C-${seq}`,
            displayName: `客户${seq}`,
        },
        settlementParty: { partyId: `p-${seq}`, displayName: `主体${seq}` },
        status: 'EFFECTIVE',
        statusLabel: '生效',
        statusTone: 'success',
        revisionNo: 1,
        validFrom: '2026-01-01',
        validTo: '9999-12-31',
        expiringWithin30Days: false,
        salesOrderCount: 0,
        activeSalesOrderCount: 0,
        ownerLabel: `负责人${seq}`,
        ownerKind: 'current_customer_owner',
        allowedActions: ['PRINT'],
        actionBlockers: [],
        ...overrides,
    }
}

function ids(rows: readonly ContractListRow[]): string[] {
    return rows.map((r) => r.contractId)
}

describe('sortRows', () => {
    it('does not mutate the input array', () => {
        const rows = [row({ validTo: '2030-01-01' }), row({ validTo: '2028-01-01' })]
        const original = [...rows]
        sortRows(rows, [])
        expect(rows).toEqual(original)
    })

    it('default-sorts by expiring flag first then validTo ascending', () => {
        const rows = [
            row({ validTo: '2030-01-01' }),
            row({ expiringWithin30Days: true, validTo: '2035-01-01' }),
            row({ validTo: '2028-01-01' }),
        ]
        expect(ids(sortRows(rows, []))).toEqual([
            rows[1].contractId,
            rows[2].contractId,
            rows[0].contractId,
        ])
    })

    it.each([
        ['contractNo', 'CT-1', 'CT-10'],
        ['customer', '客户1', '客户2'],
        ['settlement', '主体1', '主体2'],
        ['owner', '负责人1', '负责人2'],
    ] as const)('sorts %s ascending and descending', (column, small, large) => {
        const smallRow = column === 'contractNo'
            ? row({ contractNo: small })
            : column === 'customer'
              ? row({
                    customer: {
                        customerId: 'c1',
                        customerNo: 'C-1',
                        displayName: small,
                    },
                })
              : column === 'settlement'
                ? row({
                      settlementParty: {
                          partyId: 'p1',
                          displayName: small,
                      },
                  })
                : row({ ownerLabel: small })
        const largeRow = column === 'contractNo'
            ? row({ contractNo: large })
            : column === 'customer'
              ? row({
                    customer: {
                        customerId: 'c2',
                        customerNo: 'C-2',
                        displayName: large,
                    },
                })
              : column === 'settlement'
                ? row({
                      settlementParty: {
                          partyId: 'p2',
                          displayName: large,
                      },
                  })
                : row({ ownerLabel: large })

        const asc: SortingState = [{ id: column, desc: false }]
        const desc: SortingState = [{ id: column, desc: true }]
        expect(ids(sortRows([largeRow, smallRow], asc))).toEqual([
            smallRow.contractId,
            largeRow.contractId,
        ])
        expect(ids(sortRows([smallRow, largeRow], desc))).toEqual([
            largeRow.contractId,
            smallRow.contractId,
        ])
    })

    it('sorts numeric columns (revision, sales, validity)', () => {
        const r1 = row({ revisionNo: 1, salesOrderCount: 2, validTo: '2030-01-01' })
        const r2 = row({ revisionNo: 3, salesOrderCount: 9, validTo: '2028-01-01' })

        expect(
            ids(sortRows([r1, r2], [{ id: 'revision', desc: true }])),
        ).toEqual([r2.contractId, r1.contractId])
        expect(ids(sortRows([r1, r2], [{ id: 'sales', desc: true }]))).toEqual([
            r2.contractId,
            r1.contractId,
        ])
        expect(
            ids(sortRows([r1, r2], [{ id: 'validity', desc: false }])),
        ).toEqual([r2.contractId, r1.contractId])
    })

    it('keeps original relative order for an unknown column', () => {
        const rows = [row(), row()]
        expect(ids(sortRows(rows, [{ id: 'bogus', desc: true }]))).toEqual(
            ids(rows),
        )
    })
})
