// Temporary reproduction for the disabled submit button bug.
import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { useAppForm } from '@/components/form'
import { validateSalesOrderForm } from '@/features/sales-orders/lib/sales-order-create-validation'
import type { SalesOrderCreateFormValues } from '@/features/sales-orders/lib/sales-order-create-validation'

function ReproForm() {
    const form = useAppForm({
        defaultValues: {
            contractId: 'ct-1',
            requestedContractRevisionId: 'r-1',
            contractRevisionLabel: 'CT-1@v1',
            customerId: 'cu-1',
            customerName: '客户甲',
            settlementPartyId: 'sp-1',
            settlementEntity: '结算主体甲',
            nature: 'physical_service' as const,
            ownerUserId: 'u-1',
            ownerName: '张三',
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
        } satisfies SalesOrderCreateFormValues,
        validators: {
            onSubmit: ({ value }) => validateSalesOrderForm(value, 'SUBMIT'),
        },
        onSubmit: () => {
            // noop
        },
    })

    return (
        <form
            onSubmit={(e) => {
                e.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField name="lineItems[0].unitPriceGross">
                {(field) => (
                    <input
                        aria-label="含税单价"
                        value={field.state.value}
                        onChange={(e) => field.handleChange(e.target.value)}
                    />
                )}
            </form.AppField>
            <form.AppField name="lineItems[0].dueDate">
                {(field) => (
                    <input
                        aria-label="交付日期"
                        value={field.state.value}
                        onChange={(e) => field.handleChange(e.target.value)}
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton label="提交" />
            </form.AppForm>
            <form.Subscribe
                selector={(state) => ({
                    canSubmit: state.canSubmit,
                    isSubmitting: state.isSubmitting,
                    submissionAttempts: state.submissionAttempts,
                    isFieldsValid: state.isFieldsValid,
                    priceErrors: state.fieldMeta['lineItems[0].unitPriceGross']
                        ?.errors,
                    dateErrors: state.fieldMeta['lineItems[0].dueDate']?.errors,
                })}
            >
                {(state) => (
                    <pre data-testid="state">
                        {JSON.stringify(state, null, 2)}
                    </pre>
                )}
            </form.Subscribe>
        </form>
    )
}

describe('repro: submit button recovers after fixing line errors', () => {
    it('re-enables after filling unit price and due date', async () => {
        render(<ReproForm />)
        const button = screen.getByRole('button', {
            name: '提交',
        }) as HTMLButtonElement
        const price = screen.getByLabelText('含税单价')
        const date = screen.getByLabelText('交付日期')
        const dump = () =>
            JSON.parse(screen.getByTestId('state').textContent ?? '{}') as {
                canSubmit: boolean
                isSubmitting: boolean
                submissionAttempts: number
                isFieldsValid: boolean
                priceErrors?: unknown[]
                dateErrors?: unknown[]
            }

        console.log('initial:', dump())
        expect(button.disabled).toBe(false)

        fireEvent.click(button)
        await new Promise((r) => setTimeout(r, 50))
        console.log('after submit:', dump(), 'button.disabled =', button.disabled)
        expect(button.disabled).toBe(true)

        fireEvent.change(price, { target: { value: '100.00' } })
        await new Promise((r) => setTimeout(r, 50))
        console.log(
            'after price fix:',
            dump(),
            'button.disabled =',
            button.disabled,
        )

        fireEvent.change(date, { target: { value: '2026-09-01' } })
        await new Promise((r) => setTimeout(r, 50))
        console.log(
            'after date fix:',
            dump(),
            'button.disabled =',
            button.disabled,
        )
        expect(button.disabled).toBe(false)
    })
})
