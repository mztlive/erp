import { describe, it, expect } from 'vitest'

import {
    SECTION_NAV,
    blocker,
    can,
    collaboratorCount,
    collaboratorShortNames,
    collaboratorSummary,
    ownerLabel,
    resolveSection,
} from './customer-detail-helpers'
import type {
    CustomerAssignmentView,
    CustomerCenterView,
} from '@/features/customers/types'

function assignment(
    overrides: Partial<CustomerAssignmentView> = {},
): CustomerAssignmentView {
    return {
        id: 'a1',
        role: 'OWNER',
        userId: 'u1',
        userName: '张三',
        effectiveFrom: '2026-01-01',
        changeReason: '建立',
        version: 1,
        isCurrent: true,
        ...overrides,
    }
}

function customer(
    overrides: Partial<CustomerCenterView> = {},
): CustomerCenterView {
    return {
        customerId: 'cust-1',
        partyId: 'party-1',
        customerNo: 'C-001',
        status: 'active',
        statusLabel: { label: '启用', tone: 'success' },
        lockVersion: 1,
        partyLockVersion: 1,
        currentRevision: {
            revisionId: 'r1',
            revisionNo: 1,
            legalName: '示例贸易有限公司',
            effectiveFrom: '2026-01-01T00:00:00.000Z',
        },
        assignments: [],
        contacts: [],
        addresses: [],
        bankAccounts: [],
        metrics: {
            activeContractCount: 0,
            inProgressSalesOrderCount: 0,
            receivableBalance: null,
            overdueAmount: null,
        },
        contracts: [],
        salesOrders: [],
        freshness: { formalFactsAt: '2026-01-01T00:00:00.000Z' },
        allowedActions: [],
        actionBlockers: [],
        revisionTimeline: [],
        partitions: {
            identity: 'ok',
            contacts: 'ok',
            related: 'ok',
            settlement: 'ok',
            quality: 'ok',
            audit: 'ok',
        },
        ...overrides,
    }
}

describe('resolveSection', () => {
    it('maps every section nav id back to itself', () => {
        for (const item of SECTION_NAV) {
            expect(resolveSection(item.id)).toBe(item.id)
        }
    })

    it('falls back to overview for missing or unknown values', () => {
        expect(resolveSection(undefined)).toBe('overview')
        expect(resolveSection(null)).toBe('overview')
        expect(resolveSection('')).toBe('overview')
        expect(resolveSection('contacts')).toBe('overview')
    })
})

describe('can', () => {
    it('reflects the allowed actions list', () => {
        const target = customer({
            allowedActions: ['EDIT_CUSTOMER', 'MANAGE_ASSIGNMENTS'],
        })
        expect(can(target, 'EDIT_CUSTOMER')).toBe(true)
        expect(can(target, 'MANAGE_ASSIGNMENTS')).toBe(true)
        expect(can(target, 'UPLOAD_CONTRACT_PDF')).toBe(false)
    })
})

describe('blocker', () => {
    it('returns the blocker message for the action', () => {
        const target = customer({
            actionBlockers: [
                {
                    action: 'CREATE_SALES_ORDER',
                    code: 'DISABLED',
                    message: '客户已停用',
                },
            ],
        })
        expect(blocker(target, 'CREATE_SALES_ORDER')).toBe('客户已停用')
    })

    it('returns undefined when the action is not blocked', () => {
        const target = customer()
        expect(blocker(target, 'EDIT_CUSTOMER')).toBeUndefined()
    })
})

describe('ownerLabel', () => {
    it('returns the current owner name', () => {
        const target = customer({
            assignments: [
                assignment({ id: 'a1', userName: '张三' }),
                assignment({
                    id: 'a2',
                    role: 'COLLABORATOR',
                    userName: '李四',
                }),
            ],
        })
        expect(ownerLabel(target)).toBe('张三')
    })

    it('ignores owners that are no longer current', () => {
        const target = customer({
            assignments: [
                assignment({ id: 'a1', userName: '前任', isCurrent: false }),
            ],
        })
        expect(ownerLabel(target)).toBe('—')
    })

    it('shows a dash when there is no owner', () => {
        expect(ownerLabel(customer())).toBe('—')
    })
})

describe('collaboratorCount / collaboratorSummary / collaboratorShortNames', () => {
    it('counts only current collaborators', () => {
        const target = customer({
            assignments: [
                assignment({ id: 'a1', userName: '张三' }),
                assignment({
                    id: 'a2',
                    role: 'COLLABORATOR',
                    userName: '李四',
                }),
                assignment({
                    id: 'a3',
                    role: 'COLLABORATOR',
                    userName: '王五',
                    isCurrent: false,
                }),
            ],
        })
        expect(collaboratorCount(target)).toBe(1)
    })

    it('summarizes collaborators with their validity windows', () => {
        const target = customer({
            assignments: [
                assignment({
                    id: 'a2',
                    role: 'COLLABORATOR',
                    userName: '李四',
                    effectiveFrom: '2026-02-01',
                    effectiveTo: '2026-12-31',
                }),
                assignment({
                    id: 'a3',
                    role: 'COLLABORATOR',
                    userName: '王五',
                    effectiveFrom: '2026-06-01',
                }),
            ],
        })
        expect(collaboratorSummary(target)).toBe(
            '李四（2026-02-01 ~ 2026-12-31）；王五（2026-06-01 起）',
        )
        expect(collaboratorShortNames(target)).toBe('李四、王五')
    })

    it('returns the empty-state labels when there are no collaborators', () => {
        const target = customer({
            assignments: [assignment({ id: 'a1', userName: '张三' })],
        })
        expect(collaboratorCount(target)).toBe(0)
        expect(collaboratorSummary(target)).toBe('无有效协作')
        expect(collaboratorShortNames(target)).toBe('无')
    })
})
