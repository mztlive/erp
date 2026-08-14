import { describe, it, expect } from 'vitest'

import {
    emptyMetrics,
    mapAddress,
    mapAddressInput,
    mapAddressTypeFromBackend,
    mapAddressTypeToBackend,
    mapAssignment,
    mapBank,
    mapBankInput,
    mapContact,
    mapContactInput,
    mapContractSummary,
    mapCustomerStatus,
    mapDirectoryItem,
    mapMutationResult,
    mapSalesOrderSummary,
    moneyCents,
    receivableProjection,
    tsToIso,
} from './mappers'
import type {
    BackendAddress,
    BackendAssignment,
    BackendBankAccount,
    BackendContact,
    BackendCustomerView,
    BackendProfileMutation,
    BackendReceivableAccount,
    BackendSensitiveField,
} from './wire-types'

function businessDate(offsetDays: number): string {
    const date = new Date(Date.now() + offsetDays * 86_400_000)
    const pad = (value: number) => String(value).padStart(2, '0')
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function baseView(): BackendCustomerView {
    return {
        id: 'c1',
        party_id: 'p1',
        customer_no: 'C-1',
        status: 'active',
        collaborator_count: 0,
        scope_tags: ['mine'],
        version: 1,
        created_at: 0,
        updated_at: 1_723_610_000,
    }
}

describe('mapCustomerStatus', () => {
    it('maps disabled and active statuses to their labels', () => {
        expect(mapCustomerStatus('disabled')).toEqual({
            status: 'disabled',
            statusLabel: { label: '停用', tone: 'neutral' },
        })
        expect(mapCustomerStatus('active')).toEqual({
            status: 'active',
            statusLabel: { label: '启用', tone: 'success' },
        })
        expect(mapCustomerStatus('anything-else')).toEqual({
            status: 'active',
            statusLabel: { label: '启用', tone: 'success' },
        })
    })
})

describe('address type mapping', () => {
    it('maps backend codes to Chinese labels and back', () => {
        expect(mapAddressTypeFromBackend('registered')).toBe('注册地址')
        expect(mapAddressTypeFromBackend('operating')).toBe('经营地址')
        expect(mapAddressTypeFromBackend('fulfillment')).toBe('履约地址')

        expect(mapAddressTypeToBackend('注册地址')).toBe('registered')
        expect(mapAddressTypeToBackend('registered')).toBe('registered')
        expect(mapAddressTypeToBackend('经营地址')).toBe('operating')
        expect(mapAddressTypeToBackend('其他')).toBe('fulfillment')
    })
})

describe('mapDirectoryItem', () => {
    it('prefers trimmed legal name and falls back to party_no then customer_no', () => {
        const row = { ...baseView(), legal_name: '  客户甲  ' }
        expect(mapDirectoryItem(row).legalName).toBe('客户甲')

        const partyRow = {
            ...baseView(),
            legal_name: null,
            party_no: ' PN-1 ',
        }
        expect(mapDirectoryItem(partyRow).legalName).toBe('PN-1')

        const bareRow = { ...baseView(), legal_name: '', party_no: '' }
        expect(mapDirectoryItem(bareRow).legalName).toBe('C-1')
    })

    it('maps status, owner fallback, metrics and ISO updated time', () => {
        const item = mapDirectoryItem({
            ...baseView(),
            status: 'disabled',
            owner_user_name: '张三',
            collaborator_count: 3,
        })
        expect(item.statusLabel).toEqual({ label: '停用', tone: 'neutral' })
        expect(item.ownerName).toBe('张三')
        expect(item.collaboratorCount).toBe(3)
        expect(item.metrics).toEqual(emptyMetrics())
        expect(item.updatedAt).toBe(
            new Date(1_723_610_000 * 1000).toISOString(),
        )
    })

    it('falls back to the owner id when the name is missing', () => {
        const item = mapDirectoryItem({
            ...baseView(),
            owner_user_name: null,
            owner_user_id: 'u9',
        })
        expect(item.ownerName).toBe('u9')
    })
})

describe('tsToIso', () => {
    it('converts seconds to ISO time and tolerates missing values', () => {
        expect(tsToIso(0)).toBe(new Date(0).toISOString())
        expect(tsToIso(undefined)).toBe(new Date().toISOString())
        expect(tsToIso(null)).toBe(new Date().toISOString())
    })
})

describe('mapContact', () => {
    const contact: BackendContact = {
        id: 'ct-1',
        contact_name: '张三',
        title: null,
        mobile_masked: '138****0000',
        valid_from: '2026-01-01',
        is_default: true,
    }

    it('uses the sensitive token value when present', () => {
        const fields = new Map<string, BackendSensitiveField>([
            [
                'contact_mobile:ct-1',
                {
                    kind: 'contact_mobile',
                    record_id: 'ct-1',
                    masked_value: '138****8888',
                    reveal_token: 'tok-1',
                    expires_at: 0,
                },
            ],
        ])
        const view = mapContact(contact, fields)
        expect(view.phoneMasked).toBe('138****8888')
        expect(view.phoneRevealToken).toBe('tok-1')
        expect(view.fieldVisibility).toEqual({ phone: 'masked' })
        expect(view.isDefault).toBe(true)
    })

    it('falls back to the masked mobile when no token is indexed', () => {
        const view = mapContact(contact, new Map())
        expect(view.phoneMasked).toBe('138****0000')
        expect(view.phoneRevealToken).toBeUndefined()
    })
})

describe('mapAddress / mapBank', () => {
    const address: BackendAddress = {
        id: 'ad-1',
        address_type: 'registered',
        valid_from: '2026-01-01',
        is_default: false,
    }
    const bank: BackendBankAccount = {
        id: 'bk-1',
        bank_account_no: 'B-1',
        account_name: '客户甲',
        bank_name: '示例银行',
        account_number_masked: '****0001',
        valid_from: '2026-01-01',
        is_default: false,
    }

    it('maps address type and masked value with token overrides', () => {
        expect(mapAddress(address, new Map()).addressMasked).toBe('********')
        const fields = new Map<string, BackendSensitiveField>([
            [
                'address:ad-1',
                {
                    kind: 'address',
                    record_id: 'ad-1',
                    masked_value: '某市某区**',
                    reveal_token: 'tok-a',
                    expires_at: 0,
                },
            ],
        ])
        const view = mapAddress(address, fields)
        expect(view.addressType).toBe('注册地址')
        expect(view.addressMasked).toBe('某市某区**')
        expect(view.addressRevealToken).toBe('tok-a')
    })

    it('maps bank accounts with token overrides', () => {
        const view = mapBank(bank, new Map())
        expect(view.internalNo).toBe('B-1')
        expect(view.accountMasked).toBe('****0001')
    })
})

describe('mapAssignment', () => {
    const base: BackendAssignment = {
        id: 'as-1',
        customer_id: 'c1',
        user_id: 'u1',
        user_name: '李四',
        assignment_role: 'OWNER',
        valid_from: businessDate(-10),
        change_reason: '换任',
        version: 2,
        created_at: 0,
    }

    it('marks current assignments without an end date', () => {
        expect(mapAssignment(base).isCurrent).toBe(true)
    })

    it('marks ended assignments as not current', () => {
        expect(
            mapAssignment({ ...base, valid_to: businessDate(-1) }).isCurrent,
        ).toBe(false)
    })

    it('marks future assignments as not current yet', () => {
        expect(
            mapAssignment({
                ...base,
                valid_from: businessDate(1),
                valid_to: null,
            }).isCurrent,
        ).toBe(false)
    })

    it('falls back to the user id for the display name', () => {
        const view = mapAssignment({ ...base, user_name: '' })
        expect(view.userName).toBe('u1')
    })
})

describe('mapContractSummary / mapSalesOrderSummary', () => {
    it('maps known contract statuses and falls back for unknown ones', () => {
        expect(
            mapContractSummary({
                id: 'ct-1',
                contract_no: 'CT-1',
                customer_id: 'c1',
                status: 'EFFECTIVE',
            }),
        ).toEqual({
            id: 'ct-1',
            number: 'CT-1',
            title: 'CT-1',
            status: { label: '生效', tone: 'success' },
            href: '/sales/contracts/ct-1',
        })
        expect(
            mapContractSummary({
                id: 'ct-2',
                contract_no: 'CT-2',
                customer_id: 'c1',
                status: 'WEIRD',
            }).status,
        ).toEqual({ label: 'WEIRD', tone: 'neutral' })
    })

    it('maps sales order commercial statuses', () => {
        const row = (status: string) => ({
            id: 'so-1',
            order_no: 'SO-1',
            customer_id: 'c1',
            commercial_status: status,
            close_status: '',
            created_at: 0,
        })
        expect(mapSalesOrderSummary(row('EFFECTIVE')).status).toEqual({
            label: '已生效',
            tone: 'success',
        })
        expect(mapSalesOrderSummary(row('PENDING_REVIEW')).status).toEqual({
            label: '审核中',
            tone: 'warning',
        })
        expect(mapSalesOrderSummary(row('VOIDED')).status).toEqual({
            label: '已作废',
            tone: 'neutral',
        })
        expect(mapSalesOrderSummary(row('DRAFT')).status).toEqual({
            label: '草稿',
            tone: 'neutral',
        })
    })
})

describe('moneyCents / receivableProjection', () => {
    it('converts decimal strings to cents without floating point math', () => {
        expect(moneyCents('123.45')).toBe(BigInt(12345))
        expect(moneyCents('-1.20')).toBe(BigInt(-120))
        expect(moneyCents('0')).toBe(BigInt(0))
        expect(moneyCents('9')).toBe(BigInt(900))
        // 超过两位小数的金额直接拒绝，不做静默舍入
        expect(() => moneyCents('1.005')).toThrow('DECIMAL_SCALE_EXCEEDED')
    })

    it('sums open totals and only overdue, unsettled increase entries', () => {
        const accounts: BackendReceivableAccount[] = [
            {
                open_total: '100.50',
                gross_total: '0',
                settled_total: '0',
                open_invoiceable_total: '0',
                entries: [
                    {
                        direction: 'increase',
                        amount: '60.00',
                        offset_total: '10.00',
                        due_date: '2000-01-01',
                    },
                    {
                        direction: 'decrease',
                        amount: '60.00',
                        offset_total: '0',
                        due_date: '2000-01-01',
                    },
                    {
                        direction: 'increase',
                        amount: '10.00',
                        offset_total: '10.00',
                        due_date: '2000-01-01',
                    },
                    {
                        direction: 'increase',
                        amount: '5.00',
                        offset_total: '0',
                        due_date: '2999-01-01',
                    },
                    {
                        direction: 'increase',
                        amount: '5.00',
                        offset_total: '0',
                        due_date: '',
                    },
                ],
            },
            {
                open_total: '20.00',
                gross_total: '0',
                settled_total: '0',
                open_invoiceable_total: '30.00',
                entries: [
                    {
                        direction: 'increase',
                        amount: '8.00',
                        offset_total: '2.00',
                        due_date: '2000-01-02',
                    },
                ],
            },
        ]
        const projection = receivableProjection(accounts)
        expect(projection.receivableBalance).toBe('120.50')
        expect(projection.overdueAmount).toBe('56.00')
        expect(projection.earliestOverdueDate).toBe('2000-01-01')
        expect(projection.collectionProgressLabel).toBe('存在未结清余额')
        expect(projection.invoicingProgressLabel).toBe('存在可开票余额')
    })

    it('reports settled and fully invoiced states for empty accounts', () => {
        const projection = receivableProjection([])
        expect(projection.receivableBalance).toBe('0.00')
        expect(projection.overdueAmount).toBe('0.00')
        expect(projection.earliestOverdueDate).toBeUndefined()
        expect(projection.collectionProgressLabel).toBe('已结清')
        expect(projection.invoicingProgressLabel).toBe('已完成')
    })
})

describe('command input mapping', () => {
    it('trims contact fields', () => {
        expect(
            mapContactInput({
                name: ' 张三 ',
                title: ' 经理 ',
                phone: ' 138 ',
                telephone: ' ',
                email: ' z@x.com ',
                isDefault: true,
            }),
        ).toEqual({
            existing_id: undefined,
            contact_name: '张三',
            title: '经理',
            mobile: '138',
            telephone: undefined,
            email: 'z@x.com',
            is_default: true,
        })
    })

    it('maps address and bank inputs to wire fields', () => {
        expect(
            mapAddressInput({
                addressType: '注册地址',
                contactName: '张三',
                address: ' 某市某区 ',
                isDefault: false,
            }),
        ).toEqual({
            existing_id: undefined,
            address_type: 'registered',
            contact_name: '张三',
            address: '某市某区',
            is_default: false,
        })

        expect(
            mapBankInput({
                accountName: '客户甲',
                bankName: '示例银行',
                branchName: ' 分行 ',
                accountNumber: ' 1234 ',
                isDefault: false,
            }),
        ).toEqual({
            existing_id: undefined,
            account_name: '客户甲',
            bank_name: '示例银行',
            bank_branch_name: '分行',
            account_number: '1234',
            is_default: false,
        })
    })
})

describe('mapMutationResult', () => {
    it('maps the backend profile mutation into the succeeded outcome', () => {
        const backend: BackendProfileMutation = {
            customer_id: 'c1',
            customer_no: 'C-1',
            party_id: 'p1',
            revision_id: 'r2',
            revision_no: 2,
            customer_version: 3,
            party_version: 4,
            effective_from: '2026-08-14',
            recorded_at: 1_723_610_000,
            change_reason: '首版建档',
        }
        expect(mapMutationResult(backend)).toEqual({
            outcome: 'succeeded',
            customerId: 'c1',
            customerNo: 'C-1',
            revisionNo: 2,
            lockVersion: 3,
            occurredAt: new Date(1_723_610_000 * 1000).toISOString(),
            reference: 'C-1-R2',
        })
    })
})
