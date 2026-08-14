// Temporary reproduction for the disabled submit button bug on the real form.
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { SalesOrderCreateForm } from '@/features/sales-orders/components/sales-order-create-form'
import type { SalesOrderDraftResumeData } from '@/features/sales-orders/api/sales-orders-create'

vi.mock('next/navigation', () => ({
    useRouter: () => ({ push: vi.fn(), replace: vi.fn(), prefetch: vi.fn() }),
}))

vi.mock('@/features/contracts/contract-upload-dialog', () => ({
    ContractUploadDialog: () => null,
}))

vi.mock('@/features/entity-selectors', () => ({
    entitySelectorKeys: { all: ['entity-selectors'] as const },
    ContractSearchCombobox: () => null,
    MallSearchCombobox: () => null,
    SellableSkuSearchCombobox: () => null,
    VoucherCategorySearchCombobox: () => null,
}))

vi.mock('@/features/contracts/queries', () => ({
    useContractCenterQuery: () => ({
        data: {
            contractId: 'ct-1',
            contractNo: 'HT-2026-001',
            customer: { id: 'cu-1', displayName: '客户甲' },
            currentRevision: {
                revisionId: 'r-1',
                revisionNo: 1,
                settlementParty: { id: 'sp-1', displayName: '结算主体甲' },
                paymentTermSnapshot: { label: 'POSTPAY_NET30' },
            },
            revisionTimeline: [
                { revisionId: 'r-1', revisionNo: 1, isCurrent: true },
            ],
        },
        isFetching: false,
        isPending: false,
        isError: false,
        error: null,
    }),
}))

vi.mock('@/features/auth/queries', () => ({
    useAccountProfileQuery: () => ({
        data: { userid: 'u-1', name: '张三', account: 'zhangsan' },
        isPending: false,
        isError: false,
        error: null,
    }),
}))

// The real DatePicker renders a popover calendar that does not mount in jsdom;
// replace it with a button that fires the same onValueChange contract so the
// real DateField (handleChange + handleBlur) stays under test.
vi.mock('@/components/ui/date-picker', () => ({
    DatePicker: ({
        value,
        onValueChange,
        placeholder = '选择日期',
    }: {
        value?: string
        onValueChange?: (value?: string) => void
        placeholder?: string
    }) => (
        <button
            type="button"
            aria-label={value ? `已选日期 ${value}` : placeholder}
            onClick={() => onValueChange?.(value ? undefined : '2026-09-01')}
        >
            {value ?? placeholder}
        </button>
    ),
    DateTimeLocalPicker: () => null,
}))

const draft: SalesOrderDraftResumeData = {
    salesOrderId: 'so-1',
    documentNumber: 'SO-2026-001',
    version: 1,
    contractId: 'ct-1',
    nature: 'physical_service',
    welfareScene: 'ANNUAL_GIFT_BAG',
    paymentTerms: 'POSTPAY_NET30',
    fulfillmentDeadline: '2026-09-30',
    targetMallId: '',
    receivableDueDate: '',
    taxRatePercent: '13.00',
    remark: '',
    lineItems: [
        {
            rowKey: 'l1',
            name: '货物',
            sku: 'sku-1',
            skuRevisionId: 'sr-1',
            quantity: '1',
            unit: '件',
            unitPriceGross: '0.00',
            fulfillmentMode: '公司仓发',
            dueDate: '',
            faceValue: '',
            giftRate: '',
            cardForm: '',
        },
    ],
}

let queryClient: QueryClient

function renderForm() {
    queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    })
    return render(
        <QueryClientProvider client={queryClient}>
            <SalesOrderCreateForm initialDraft={draft} purpose="create" />
        </QueryClientProvider>,
    )
}

afterEach(() => {
    queryClient?.clear()
})

describe('real SalesOrderCreateForm recovery', () => {
    it('re-enables submit after fixing unit price and due date', async () => {
        const { container } = renderForm()
        const button = screen.getByRole('button', {
            name: '提交',
        }) as HTMLButtonElement

        await waitFor(() => {
            expect(button.disabled).toBe(false)
        })
        expect(button.disabled).toBe(false)

        fireEvent.click(button)
        await waitFor(() => {
            expect(button.disabled).toBe(true)
        })
        expect(screen.getByText('含税单价必须大于 0')).toBeTruthy()
        expect(screen.getByText('请选择明细交付日期')).toBeTruthy()

        // Fix the price (0.00 -> 100.00)
        const priceInput = screen.getByLabelText('含税单价')
        fireEvent.change(priceInput, { target: { value: '100.00' } })
        await waitFor(() => {
            expect(screen.queryByText('含税单价必须大于 0')).toBeNull()
        })

        // Fix the due date via the DatePicker mock (fires DateField's
        // handleChange + handleBlur path)
        const dateTrigger = screen.getByRole('button', {
            name: '选择日期',
        }) as HTMLButtonElement
        fireEvent.click(dateTrigger)
        await waitFor(() => {
            expect(screen.queryByText('请选择明细交付日期')).toBeNull()
        })

        await waitFor(() => {
            expect(button.disabled).toBe(false)
        }, { timeout: 3000 })
        expect(button.disabled).toBe(false)
    })
})
