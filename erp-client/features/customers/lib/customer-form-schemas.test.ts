import { describe, it, expect } from 'vitest'

import {
    createSchema,
    editSchema,
} from '@/features/customers/lib/customer-form-schemas'

const validCreate = {
    legalName: '客户甲有限公司',
    shortName: '客户甲',
    unifiedCreditCode: '91330000123456789X',
    defaultPaymentTerm: '',
    status: 'active',
    changeReason: '',
    contacts: [],
    addresses: [],
    bankAccounts: [],
}

describe('createSchema', () => {
    it('accepts a minimal valid customer', () => {
        expect(createSchema.safeParse(validCreate).success).toBe(true)
    })

    it('rejects short legal names', () => {
        const result = createSchema.safeParse({
            ...validCreate,
            legalName: '甲',
        })
        expect(result.success).toBe(false)
    })

    it('requires an 18-char alphanumeric unified credit code', () => {
        expect(
            createSchema.safeParse({
                ...validCreate,
                unifiedCreditCode: '12345',
            }).success,
        ).toBe(false)
        expect(
            createSchema.safeParse({
                ...validCreate,
                unifiedCreditCode: '91330000123456789!',
            }).success,
        ).toBe(false)
        expect(
            createSchema.safeParse({
                ...validCreate,
                unifiedCreditCode: '',
            }).success,
        ).toBe(false)
    })

    it('requires a phone for brand-new contacts but not for existing ones', () => {
        expect(
            createSchema.safeParse({
                ...validCreate,
                contacts: [
                    {
                        name: '张三',
                        title: '',
                        phone: '',
                        telephone: '',
                        email: '',
                        isDefault: true,
                    },
                ],
            }).success,
        ).toBe(false)

        expect(
            createSchema.safeParse({
                ...validCreate,
                contacts: [
                    {
                        existingId: 'ct-1',
                        name: '张三',
                        title: '',
                        phone: '',
                        telephone: '',
                        email: '',
                        isDefault: true,
                    },
                ],
            }).success,
        ).toBe(true)
    })

    it('requires an address for brand-new addresses and an account number for new banks', () => {
        expect(
            createSchema.safeParse({
                ...validCreate,
                addresses: [
                    {
                        addressType: '注册地址',
                        contactName: '',
                        address: '',
                        isDefault: true,
                    },
                ],
            }).success,
        ).toBe(false)

        expect(
            createSchema.safeParse({
                ...validCreate,
                bankAccounts: [
                    {
                        accountName: '客户甲',
                        bankName: '示例银行',
                        branchName: '',
                        accountNumber: '',
                        isDefault: true,
                    },
                ],
            }).success,
        ).toBe(false)
    })

    it('rejects unknown status values', () => {
        expect(
            createSchema.safeParse({
                ...validCreate,
                status: 'archived',
            }).success,
        ).toBe(false)
    })
})

describe('editSchema', () => {
    const validEdit = {
        ...validCreate,
        changeReason: '更新资料',
    }

    it('accepts a valid edit payload', () => {
        expect(editSchema.safeParse(validEdit).success).toBe(true)
    })

    it('requires a revision reason of at least two characters', () => {
        expect(
            editSchema.safeParse({ ...validEdit, changeReason: '改' }).success,
        ).toBe(false)
        expect(
            editSchema.safeParse({ ...validEdit, changeReason: '  ' }).success,
        ).toBe(false)
    })
})
