import { describe, it, expect } from 'vitest'

import {
    computeContractMetrics,
    contractMetricLabel,
    filterContracts,
} from '@/features/contracts/lib/filter-contracts'
import type { ContractListRow } from '@/features/contracts/types'

let seq = 0
function row(overrides: Partial<ContractListRow> = {}): ContractListRow {
    seq += 1
    return {
        contractId: `ct-${seq}`,
        contractNo: `CT-2026-00${seq}`,
        customer: {
            customerId: `c-${seq}`,
            customerNo: `C-${seq}`,
            displayName: `客户${seq}`,
        },
        settlementParty: { partyId: `p-${seq}`, displayName: `结算主体${seq}` },
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

describe('filterContracts', () => {
    it('passes through all rows with empty search and all filters', () => {
        const rows = [row(), row({ status: 'EXPIRED', statusLabel: '到期', statusTone: 'warning' })]
        expect(
            filterContracts(rows, {
                search: '',
                metricKey: 'all',
                statusFilter: 'all',
            }),
        ).toEqual(rows)
    })

    it('matches search across contract no, customer, party and owner', () => {
        const target = row({
            contractNo: 'CT-X9',
            customer: {
                customerId: 'c-x',
                customerNo: 'CX-01',
                displayName: '华东贸易',
            },
            settlementParty: { partyId: 'p-x', displayName: '华东结算' },
            ownerLabel: '李四',
        })
        const others = [row(), row()]

        const all = [target, ...others]
        expect(
            filterContracts(all, {
                search: 'x9',
                metricKey: 'all',
                statusFilter: 'all',
            }),
        ).toEqual([target])
        expect(
            filterContracts(all, {
                search: '华东',
                metricKey: 'all',
                statusFilter: 'all',
            }),
        ).toEqual([target])
        expect(
            filterContracts(all, {
                search: '李四',
                metricKey: 'all',
                statusFilter: 'all',
            }),
        ).toEqual([target])
        expect(
            filterContracts(all, {
                search: '不存在',
                metricKey: 'all',
                statusFilter: 'all',
            }),
        ).toEqual([])
    })

    it('filters by metric key', () => {
        const effective = row()
        const expiring = row({ expiringWithin30Days: true })
        const expired = row({
            status: 'EXPIRED',
            statusLabel: '到期',
            statusTone: 'warning',
        })
        const terminated = row({
            status: 'TERMINATED',
            statusLabel: '终止',
            statusTone: 'neutral',
        })
        const all = [effective, expiring, expired, terminated]

        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'effective',
                statusFilter: 'all',
            }).map((r) => r.contractId),
        ).toEqual([effective.contractId, expiring.contractId])
        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'expiring_30d',
                statusFilter: 'all',
            }).map((r) => r.contractId),
        ).toEqual([expiring.contractId])
        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'expired',
                statusFilter: 'all',
            }).map((r) => r.contractId),
        ).toEqual([expired.contractId])
        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'terminated',
                statusFilter: 'all',
            }).map((r) => r.contractId),
        ).toEqual([terminated.contractId])
    })

    it('filters by settlement party id and owner', () => {
        const target = row({
            settlementParty: { partyId: 'p-x', displayName: '华东结算' },
            ownerLabel: '李四',
        })
        const others = [row(), row()]
        const all = [target, ...others]

        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'all',
                statusFilter: 'all',
                settlementPartyId: 'p-x',
            }),
        ).toEqual([target])
        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'all',
                statusFilter: 'all',
                owner: '李四',
            }),
        ).toEqual([target])
        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'all',
                statusFilter: 'all',
                settlementPartyId: 'p-x',
                owner: '李四',
            }),
        ).toEqual([target])
        expect(
            filterContracts(all, {
                search: '',
                metricKey: 'all',
                statusFilter: 'all',
                owner: '不存在的人',
            }),
        ).toEqual([])
    })

    it('filters by status', () => {
        const effective = row()
        const terminated = row({
            status: 'TERMINATED',
            statusLabel: '终止',
            statusTone: 'neutral',
        })
        expect(
            filterContracts([effective, terminated], {
                search: '',
                metricKey: 'all',
                statusFilter: 'TERMINATED',
            }),
        ).toEqual([terminated])
    })

    it('handles empty input', () => {
        expect(
            filterContracts([], {
                search: 'x',
                metricKey: 'all',
                statusFilter: 'all',
            }),
        ).toEqual([])
    })
})

describe('computeContractMetrics', () => {
    it('counts rows by status and expiring flag', () => {
        const rows = [
            row(),
            row({ expiringWithin30Days: true }),
            row({
                status: 'EXPIRED',
                statusLabel: '到期',
                statusTone: 'warning',
            }),
            row({
                status: 'TERMINATED',
                statusLabel: '终止',
                statusTone: 'neutral',
            }),
            row({
                status: 'EXPIRED',
                statusLabel: '到期',
                statusTone: 'warning',
            }),
        ]
        expect(computeContractMetrics(rows)).toEqual({
            all: 5,
            effective: 2,
            expiring_30d: 1,
            expired: 2,
            terminated: 1,
        })
    })

    it('returns zeros for empty input', () => {
        expect(computeContractMetrics([])).toEqual({
            all: 0,
            effective: 0,
            expiring_30d: 0,
            expired: 0,
            terminated: 0,
        })
    })
})

describe('contractMetricLabel', () => {
    it('maps metric keys to labels with a default', () => {
        expect(contractMetricLabel('all')).toBe('全部')
        expect(contractMetricLabel('effective')).toBe('有效')
        expect(contractMetricLabel('expiring_30d')).toBe('30 天内到期')
        expect(contractMetricLabel('expired')).toBe('已到期')
        expect(contractMetricLabel('terminated')).toBe('已终止')
    })
})
