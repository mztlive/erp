import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

import {
    CONTRACT_SECTION_NAV,
    contractSectionHref,
    isExpiringWithin30Days,
    resolveSection,
} from '@/features/contracts/lib/contract-detail-helpers'
import type { ContractCenterView } from '@/features/contracts/types'

function revision(
    validTo: string,
): ContractCenterView['currentRevision'] {
    return {
        revisionId: 'r1',
        revisionNo: 1,
        settlementParty: { id: 'p1', displayName: '主体乙' },
        paymentTermSnapshot: {
            label: '合同约定',
            description: '合同约定',
        },
        invoiceRequirementSnapshot: {
            titleType: '增值税专用发票',
            contentSummary: '税点 13',
        },
        validFrom: '2026-01-01',
        validTo,
        termsSummary: '合同约定',
    }
}

function center(overrides: Partial<ContractCenterView> = {}): ContractCenterView {
    return {
        contractId: 'ct-1',
        contractNo: 'CT-1',
        status: 'EFFECTIVE',
        statusLabel: '生效',
        statusTone: 'success',
        lockVersion: 1,
        customer: { id: 'c1', displayName: '客户甲' },
        ownerLabel: '张三',
        ownerKind: 'current_customer_owner',
        currentRevision: revision('9999-12-31'),
        attachments: [],
        relatedSalesOrders: [],
        revisionTimeline: [],
        auditTimeline: [],
        allowedActions: ['PRINT'],
        actionBlockers: [],
        sourceAsOf: '2026-01-01T00:00:00.000Z',
        relatedSalesOrdersAsOf: '2026-01-01T00:00:00.000Z',
        queriedAt: '2026-01-01T00:00:00.000Z',
        selectableForNewSalesOrder: true,
        ...overrides,
    }
}

describe('resolveSection', () => {
    it('maps known section names and defaults to overview', () => {
        expect(resolveSection('settlement')).toBe('settlement')
        expect(resolveSection('attachments')).toBe('attachments')
        expect(resolveSection('sales-orders')).toBe('sales-orders')
        expect(resolveSection('versions')).toBe('versions')
        expect(resolveSection('overview')).toBe('overview')
        expect(resolveSection(undefined)).toBe('overview')
        expect(resolveSection('bogus')).toBe('overview')
    })

    it('exposes stable section nav labels', () => {
        expect(CONTRACT_SECTION_NAV.map((item) => item.id)).toEqual([
            'overview',
            'settlement',
            'attachments',
            'sales-orders',
            'versions',
        ])
    })

    it('builds section hrefs without query for overview', () => {
        expect(contractSectionHref('ct-1', 'overview')).toBe(
            '/sales/contracts/ct-1',
        )
        expect(contractSectionHref('ct-1', 'settlement')).toBe(
            '/sales/contracts/ct-1?section=settlement',
        )
    })
})

describe('isExpiringWithin30Days', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date(2026, 5, 1, 12, 0, 0))
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('is true for an effective contract ending within 30 days', () => {
        expect(
            isExpiringWithin30Days(
                center({ currentRevision: revision('2026-06-30') }),
            ),
        ).toBe(true)
    })

    it('is false when validTo is beyond 30 days', () => {
        expect(
            isExpiringWithin30Days(
                center({ currentRevision: revision('2026-12-31') }),
            ),
        ).toBe(false)
    })

    it('is false when the contract is not effective', () => {
        expect(
            isExpiringWithin30Days(
                center({
                    status: 'EXPIRED',
                    statusLabel: '到期',
                    statusTone: 'warning',
                    currentRevision: revision('2026-06-30'),
                }),
            ),
        ).toBe(false)
    })

    it('is false for an already passed validTo and for an invalid date', () => {
        expect(
            isExpiringWithin30Days(
                center({ currentRevision: revision('2026-05-01') }),
            ),
        ).toBe(false)
        expect(
            isExpiringWithin30Days(
                center({ currentRevision: revision('not-a-date') }),
            ),
        ).toBe(false)
    })
})
