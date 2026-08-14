import { describe, it, expect, vi, afterEach } from 'vitest'

import {
    buildDefaults,
    buildFormSubmission,
    editableValue,
    newIdempotencyKey,
    type FormValues,
} from './customer-form-values'
import type { CustomerCenterView } from '@/features/customers/types'

function customer(
    overrides: Partial<CustomerCenterView> = {},
): CustomerCenterView {
    return {
        customerId: 'cust-1',
        partyId: 'party-1',
        customerNo: 'C-001',
        status: 'active',
        statusLabel: { label: '启用', tone: 'success' },
        lockVersion: 5,
        partyLockVersion: 4,
        currentRevision: {
            revisionId: 'r2',
            revisionNo: 2,
            legalName: '示例贸易有限公司',
            shortName: '示例贸易',
            unifiedCreditCode: '91310000XXXXXXXXXX',
            defaultPaymentTerm: 'POSTPAY_NET30',
            effectiveFrom: '2026-06-01T00:00:00.000Z',
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
        freshness: { formalFactsAt: '2026-06-01T00:00:00.000Z' },
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

function formValues(overrides: Partial<FormValues> = {}): FormValues {
    return {
        legalName: '示例贸易有限公司',
        shortName: '示例贸易',
        unifiedCreditCode: '91310000XXXXXXXXXX',
        defaultPaymentTerm: 'POSTPAY_NET30',
        status: 'active',
        changeReason: '更新资料',
        contacts: [],
        addresses: [],
        bankAccounts: [],
        ...overrides,
    }
}

describe('newIdempotencyKey', () => {
    afterEach(() => {
        vi.restoreAllMocks()
    })

    it('prefixes the key and varies per call', () => {
        vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000)

        const first = newIdempotencyKey('create')
        expect(first).toMatch(/^create-/)
        expect(first).not.toBe(newIdempotencyKey('create'))
    })
})

describe('editableValue', () => {
    it('returns empty for reveal tokens', () => {
        expect(editableValue('token-1', '138****0000')).toBe('')
    })

    it('returns empty for masked placeholders', () => {
        expect(editableValue(undefined, '')).toBe('')
        expect(editableValue(undefined, '—')).toBe('')
        expect(editableValue(undefined, '138****0000')).toBe('')
    })

    it('returns the plain value when it is not masked', () => {
        expect(editableValue(undefined, '上海市徐汇区 1 号')).toBe(
            '上海市徐汇区 1 号',
        )
    })
})

describe('buildDefaults', () => {
    it('produces the empty create-mode defaults', () => {
        expect(buildDefaults('create', undefined)).toEqual({
            legalName: '',
            shortName: '',
            unifiedCreditCode: '',
            defaultPaymentTerm: 'POSTPAY_NET30',
            status: 'active',
            changeReason: '',
            contacts: [],
            addresses: [],
            bankAccounts: [],
        })
    })

    it('maps the current revision and sub-entities for edit mode', () => {
        const target = customer({
            contacts: [
                {
                    id: 'c1',
                    name: '张三',
                    title: '采购',
                    phoneMasked: '138****0000',
                    phoneRevealToken: 't1',
                    email: 'zhang@example.com',
                    isDefault: true,
                    effectiveFrom: '2026-01-01',
                    fieldVisibility: { phone: 'masked' },
                },
            ],
            addresses: [
                {
                    id: 'a1',
                    addressType: '注册地址',
                    addressMasked: '上海市徐汇区 ** 号',
                    contactName: '张三',
                    isDefault: true,
                    effectiveFrom: '2026-01-01',
                    fieldVisibility: { address: 'masked' },
                },
            ],
            bankAccounts: [
                {
                    id: 'b1',
                    internalNo: 'B-1',
                    accountName: '示例贸易有限公司',
                    bankName: '示例银行',
                    branchName: '徐汇支行',
                    accountMasked: '****0000',
                    accountRevealToken: 't2',
                    isDefault: true,
                    effectiveFrom: '2026-01-01',
                    fieldVisibility: { accountNumber: 'masked' },
                },
            ],
        })

        const defaults = buildDefaults('edit', target)

        expect(defaults).toEqual({
            legalName: '示例贸易有限公司',
            shortName: '示例贸易',
            unifiedCreditCode: '91310000XXXXXXXXXX',
            defaultPaymentTerm: 'POSTPAY_NET30',
            status: 'active',
            changeReason: '',
            contacts: [
                {
                    existingId: 'c1',
                    name: '张三',
                    title: '采购',
                    phone: '',
                    telephone: '',
                    email: 'zhang@example.com',
                    isDefault: true,
                },
            ],
            addresses: [
                {
                    existingId: 'a1',
                    addressType: '注册地址',
                    contactName: '张三',
                    address: '',
                    isDefault: true,
                },
            ],
            bankAccounts: [
                {
                    existingId: 'b1',
                    accountName: '示例贸易有限公司',
                    bankName: '示例银行',
                    branchName: '徐汇支行',
                    accountNumber: '',
                    isDefault: true,
                },
            ],
        })
    })

    it('fills edit defaults from optional fields with empty fallbacks', () => {
        const target = customer({
            currentRevision: {
                revisionId: 'r1',
                revisionNo: 1,
                legalName: '示例贸易有限公司',
                effectiveFrom: '2026-01-01T00:00:00.000Z',
            },
            status: 'disabled',
        })
        const defaults = buildDefaults('edit', target)

        expect(defaults.shortName).toBe('')
        expect(defaults.unifiedCreditCode).toBe('')
        expect(defaults.defaultPaymentTerm).toBe('')
        expect(defaults.status).toBe('disabled')
    })
})

describe('buildFormSubmission', () => {
    const permissions = {
        canWriteContacts: true,
        canWriteAddresses: true,
        canWriteBanks: true,
        idempotencyKey: 'create-abc',
    }

    it('builds the create input with trimmed values', () => {
        const input = buildFormSubmission(
            'create',
            formValues({
                legalName: ' 示例贸易有限公司 ',
                shortName: ' ',
                contacts: [
                    {
                        name: ' 张三 ',
                        title: ' ',
                        phone: ' 13800000000 ',
                        telephone: '',
                        email: '',
                        isDefault: true,
                    },
                ],
            }),
            undefined,
            permissions,
        )

        expect(input).toEqual({
            legalName: '示例贸易有限公司',
            shortName: undefined,
            unifiedCreditCode: '91310000XXXXXXXXXX',
            defaultPaymentTerm: 'POSTPAY_NET30',
            status: 'active',
            contacts: [
                {
                    existingId: undefined,
                    name: '张三',
                    title: undefined,
                    phone: '13800000000',
                    telephone: undefined,
                    email: undefined,
                    isDefault: true,
                },
            ],
            addresses: [],
            bankAccounts: [],
            idempotencyKey: 'create-abc',
        })
    })

    it('builds the edit input with lock versions and change reason', () => {
        const target = customer()
        const input = buildFormSubmission(
            'edit',
            formValues({ changeReason: ' 更新名称 ' }),
            target,
            permissions,
        )

        expect(input).toEqual({
            customerId: 'cust-1',
            expectedLockVersion: 5,
            expectedPartyVersion: 4,
            baseRevisionId: 'r2',
            legalName: '示例贸易有限公司',
            shortName: '示例贸易',
            unifiedCreditCode: '91310000XXXXXXXXXX',
            defaultPaymentTerm: 'POSTPAY_NET30',
            status: 'active',
            changeReason: '更新名称',
            contacts: [],
            addresses: [],
            bankAccounts: [],
            idempotencyKey: 'create-abc',
        })
    })

    it('keeps masked values out of the payload for existing rows', () => {
        const input = buildFormSubmission(
            'edit',
            formValues({
                contacts: [
                    {
                        existingId: 'c1',
                        name: '张三',
                        title: '',
                        phone: '138****0000',
                        telephone: '',
                        email: '',
                        isDefault: true,
                    },
                ],
            }),
            customer(),
            permissions,
        )

        expect((input as unknown as { contacts: unknown[] }).contacts).toEqual([
            {
                existingId: 'c1',
                name: '张三',
                title: undefined,
                phone: undefined,
                telephone: undefined,
                email: undefined,
                isDefault: true,
            },
        ])
    })

    it('omits the sub-entity arrays without write permissions', () => {
        const input = buildFormSubmission(
            'create',
            formValues({
                contacts: [
                    {
                        name: '张三',
                        title: '',
                        phone: '13800000000',
                        telephone: '',
                        email: '',
                        isDefault: true,
                    },
                ],
                addresses: [
                    {
                        addressType: '履约地址',
                        contactName: '',
                        address: '上海市徐汇区 1 号',
                        isDefault: true,
                    },
                ],
                bankAccounts: [
                    {
                        accountName: '示例贸易有限公司',
                        bankName: '示例银行',
                        branchName: '',
                        accountNumber: '6222000011110000',
                        isDefault: true,
                    },
                ],
            }),
            undefined,
            { ...permissions, canWriteBanks: false },
        )

        expect((input as unknown as { contacts: unknown[] }).contacts).toHaveLength(1)
        expect((input as unknown as { addresses: unknown[] }).addresses).toHaveLength(1)
        expect((input as unknown as { bankAccounts?: unknown[] }).bankAccounts).toBeUndefined()
    })

    it('drops masked phone values for new rows even without an existing id', () => {
        const input = buildFormSubmission(
            'create',
            formValues({
                contacts: [
                    {
                        name: '张三',
                        title: '',
                        phone: '138****0000',
                        telephone: '',
                        email: '',
                        isDefault: true,
                    },
                ],
            }),
            undefined,
            permissions,
        )

        expect((input as unknown as { contacts: unknown[] }).contacts).toEqual([
            {
                existingId: undefined,
                name: '张三',
                title: undefined,
                phone: '138****0000',
                telephone: undefined,
                email: undefined,
                isDefault: true,
            },
        ])
    })
})
