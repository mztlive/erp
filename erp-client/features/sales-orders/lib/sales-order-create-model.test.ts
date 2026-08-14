import { describe, it, expect } from 'vitest'

import {
    calculateTotals,
    createEmptyLine,
    deriveVoucherGiftPreview,
    errorMessage,
    hasMeaningfulLines,
} from '@/features/sales-orders/lib/sales-order-create-model'

const emptyLine = (overrides: Record<string, string> = {}) => ({
    rowKey: 'r1',
    name: '',
    sku: '',
    skuRevisionId: '',
    quantity: '1',
    unit: '',
    unitPriceGross: '0.00',
    fulfillmentMode: '',
    dueDate: '',
    faceValue: '',
    giftRate: '',
    cardForm: '',
    ...overrides,
})

describe('deriveVoucherGiftPreview', () => {
    it('derives gift amount and rate from face value, price and quantity', () => {
        const gift = deriveVoucherGiftPreview('100.00', '80.00', '10')
        expect(gift).toEqual({
            giftAmount: '200.00',
            giftRatePercent: '25.00',
        })
    })

    it('returns null when any input is missing', () => {
        expect(deriveVoucherGiftPreview('', '80.00', '10')).toBeNull()
        expect(deriveVoucherGiftPreview('100', '', '10')).toBeNull()
        expect(deriveVoucherGiftPreview('100', '80', '')).toBeNull()
    })

    it('returns null for a zero or invalid transaction amount', () => {
        expect(deriveVoucherGiftPreview('100.00', '0', '10')).toBeNull()
        expect(deriveVoucherGiftPreview('abc', '80.00', '10')).toBeNull()
    })
})

describe('createEmptyLine', () => {
    it('applies physical-service defaults', () => {
        const line = createEmptyLine('physical_service')
        expect(line.unit).toBe('')
        expect(line.fulfillmentMode).toBe('公司仓发')
        expect(line.cardForm).toBe('')
        expect(line.quantity).toBe('1')
        expect(line.unitPriceGross).toBe('0.00')
    })

    it('applies card-voucher defaults', () => {
        const line = createEmptyLine('card_voucher')
        expect(line.unit).toBe('张')
        expect(line.fulfillmentMode).toBe('')
        expect(line.cardForm).toBe('电子卡')
    })

    it('generates a unique rowKey per call', () => {
        const first = createEmptyLine('physical_service')
        const second = createEmptyLine('physical_service')
        expect(first.rowKey).not.toBe(second.rowKey)
    })
})

describe('hasMeaningfulLines', () => {
    it('treats pristine lines as empty', () => {
        expect(hasMeaningfulLines([emptyLine()])).toBe(false)
    })

    it('detects filled content', () => {
        expect(
            hasMeaningfulLines([emptyLine({ name: '货物' })]),
        ).toBe(true)
        expect(hasMeaningfulLines([emptyLine({ sku: 'sku-1' })])).toBe(true)
        expect(
            hasMeaningfulLines([emptyLine({ quantity: '2' })]),
        ).toBe(true)
        expect(
            hasMeaningfulLines([emptyLine({ unitPriceGross: '9.90' })]),
        ).toBe(true)
        expect(
            hasMeaningfulLines([emptyLine({ faceValue: '100' })]),
        ).toBe(true)
        expect(
            hasMeaningfulLines([emptyLine({ dueDate: '2026-09-01' })]),
        ).toBe(true)
    })

    it('treats an empty list as having no meaningful lines', () => {
        expect(hasMeaningfulLines([])).toBe(false)
    })
})

describe('calculateTotals', () => {
    it('splits gross, net and tax at the given rate', () => {
        const totals = calculateTotals(
            [emptyLine({ quantity: '2', unitPriceGross: '100.00' })],
            '13.00',
        )
        expect(totals).toEqual({
            gross: '200.00',
            net: '176.99',
            tax: '23.01',
        })
    })

    it('sums across lines', () => {
        const totals = calculateTotals(
            [
                emptyLine({ quantity: '1', unitPriceGross: '100.00' }),
                emptyLine({ quantity: '3', unitPriceGross: '50.00' }),
            ],
            '10.00',
        )
        expect(totals.gross).toBe('250.00')
    })

    it('falls back to zeros on invalid input', () => {
        expect(calculateTotals([], '')).toEqual({
            gross: '0.00',
            net: '0.00',
            tax: '0.00',
        })
        expect(calculateTotals([], 'abc')).toEqual({
            gross: '0.00',
            net: '0.00',
            tax: '0.00',
        })
    })
})

describe('errorMessage', () => {
    it('translates known api codes', () => {
        expect(
            errorMessage({
                kind: 'Validation',
                message: 'CONTRACT_NOT_SELECTABLE',
            }),
        ).toBe('所选合同已不可用于新建销售单，请刷新后重选。')
        expect(
            errorMessage({ kind: 'Validation', message: 'LINE_ITEM_REQUIRED' }),
        ).toBe('至少需要一条销售明细。')
    })

    it('passes unknown messages through', () => {
        expect(errorMessage(new Error('其他失败原因'))).toBe('其他失败原因')
    })

    it('falls back for empty errors', () => {
        expect(errorMessage(undefined)).toBe('创建失败，请重试。')
    })
})
