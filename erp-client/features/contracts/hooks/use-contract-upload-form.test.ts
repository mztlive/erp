import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import {
    uploadErrorMessage,
    uploadSchema,
    useContractUploadForm,
} from '@/features/contracts/hooks/use-contract-upload-form'
import { useUploadContractPdfMutation } from '@/features/contracts/hooks/queries'
import type { UploadContractPdfResult } from '@/features/contracts/types'

vi.mock('@/features/contracts/hooks/queries', () => ({
    useUploadContractPdfMutation: vi.fn(),
}))

vi.mock('@/features/customers/queries', () => ({
    useCustomerCenterQuery: vi.fn(),
}))

vi.mock('@/features/auth/queries', () => ({
    useAccountProfileQuery: vi.fn(),
}))

vi.mock('@/lib/permissions', () => ({
    hasPermission: vi.fn(),
}))

const { useCustomerCenterQuery } = await import(
    '@/features/customers/queries'
)
const { useAccountProfileQuery } = await import('@/features/auth/queries')
const { hasPermission } = await import('@/lib/permissions')

const mockedUploadMutation = vi.mocked(useUploadContractPdfMutation)
const mockedCustomerQuery = vi.mocked(useCustomerCenterQuery)
const mockedAccountProfile = vi.mocked(useAccountProfileQuery)
const mockedHasPermission = vi.mocked(hasPermission)

const validFile = new File(['pdf'], 'signed.pdf', {
    type: 'application/pdf',
})

const validValues = {
    pdfFile: validFile,
    contractNo: 'CT-2026-01',
    customerId: 'c1',
    customerName: '客户甲',
    settlementPartyId: 'p1',
    settlementPartyName: '主体乙',
    paymentTerms: 'CONTRACT',
    signedAt: '2026-01-01',
    validFrom: '2026-01-01',
    validTo: '2027-01-01',
}

function makeUploadMutationMock() {
    const mutateAsync = vi.fn()
    return {
        mutateAsync,
        isPending: false,
        isError: false,
        error: null,
        reset: vi.fn(),
    }
}

type UploadMutationMock = ReturnType<typeof makeUploadMutationMock>

function pad(n: number): string {
    return String(n).padStart(2, '0')
}

describe('useContractUploadForm', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        mockedCustomerQuery.mockReturnValue({
            data: null,
            isPending: false,
        } as unknown as ReturnType<typeof useCustomerCenterQuery>)
        mockedAccountProfile.mockReturnValue({
            data: { permissions: [] },
        } as unknown as ReturnType<typeof useAccountProfileQuery>)
        mockedHasPermission.mockReturnValue(false)
        mockedUploadMutation.mockReturnValue(
            makeUploadMutationMock() as unknown as ReturnType<
                typeof useUploadContractPdfMutation
            >,
        )
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('returns form state with defaults while closed', () => {
        const { result } = renderHook(() =>
            useContractUploadForm({
                open: false,
                onOpenChange: vi.fn(),
                initialCustomerId: 'c-seed',
            }),
        )

        expect(result.current.form.state.values).toMatchObject({
            pdfFile: null,
            contractNo: '',
            customerId: 'c-seed',
            paymentTerms: 'CONTRACT',
            signedAt: '',
            validFrom: '',
            validTo: '',
        })
        expect(result.current.dirty).toBe(false)
        expect(result.current.discardOpen).toBe(false)
        expect(result.current.canReadAllCustomers).toBe(false)
    })

    it('seeds signedAt/validFrom to today and validTo to one year later when opened', () => {
        const now = new Date(2026, 5, 15, 10, 0, 0)
        vi.useFakeTimers()
        vi.setSystemTime(now)

        const { result, rerender } = renderHook(
            ({ open }: { open: boolean }) =>
                useContractUploadForm({
                    open,
                    onOpenChange: vi.fn(),
                    initialCustomerId: '',
                }),
            { initialProps: { open: false } },
        )

        act(() => {
            rerender({ open: true })
        })

        const todayText = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
        const nextYearText = `${now.getFullYear() + 1}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
        expect(result.current.form.state.values.signedAt).toBe(todayText)
        expect(result.current.form.state.values.validFrom).toBe(todayText)
        expect(result.current.form.state.values.validTo).toBe(nextYearText)
        // 打开即写入默认日期，表单进入脏状态（未提交离开需确认）。
        expect(result.current.dirty).toBe(true)
    })

    it('seeds the preselected customer into the form once when opened', () => {
        mockedCustomerQuery.mockReturnValue({
            data: {
                customerId: 'c-77',
                currentRevision: { legalName: '预选客户有限公司' },
            },
            isPending: false,
        } as unknown as ReturnType<typeof useCustomerCenterQuery>)

        const { result, rerender } = renderHook(
            ({ open }: { open: boolean }) =>
                useContractUploadForm({
                    open,
                    onOpenChange: vi.fn(),
                    initialCustomerId: 'c-77',
                }),
            { initialProps: { open: false } },
        )

        act(() => {
            rerender({ open: true })
        })

        expect(result.current.form.state.values.customerId).toBe('c-77')
        expect(result.current.form.state.values.customerName).toBe(
            '预选客户有限公司',
        )
    })

    it('submits valid values through the upload mutation and closes', async () => {
        const uploaded: UploadContractPdfResult = {
            contractId: 'ct-new',
            contractNo: 'CT-2026-01',
            revisionId: 'r1',
            revisionNo: 1,
            uploadedAt: '2026-01-01T00:00:00.000Z',
            fileName: 'signed.pdf',
            reference: 'CT-UP-CT-2026-01',
        }
        const mutation: UploadMutationMock = makeUploadMutationMock()
        mutation.mutateAsync.mockResolvedValue(uploaded)
        mockedUploadMutation.mockReturnValue(
            mutation as unknown as ReturnType<
                typeof useUploadContractPdfMutation
            >,
        )

        const onOpenChange = vi.fn()
        const onSuccess = vi.fn()
        const { result, rerender } = renderHook(
            ({ open }: { open: boolean }) =>
                useContractUploadForm({
                    open,
                    onOpenChange,
                    initialCustomerId: '',
                    onSuccess,
                }),
            { initialProps: { open: false } },
        )

        act(() => {
            rerender({ open: true })
        })

        const { form } = result.current
        act(() => {
            form.setFieldValue('pdfFile', validValues.pdfFile)
            form.setFieldValue('contractNo', validValues.contractNo)
            form.setFieldValue('customerId', validValues.customerId)
            form.setFieldValue('customerName', validValues.customerName)
            form.setFieldValue(
                'settlementPartyId',
                validValues.settlementPartyId,
            )
            form.setFieldValue(
                'settlementPartyName',
                validValues.settlementPartyName,
            )
            form.setFieldValue('paymentTerms', validValues.paymentTerms)
            form.setFieldValue('signedAt', validValues.signedAt)
            form.setFieldValue('validFrom', validValues.validFrom)
            form.setFieldValue('validTo', validValues.validTo)
        })

        await act(async () => {
            await form.handleSubmit()
        })

        expect(mutation.mutateAsync).toHaveBeenCalledTimes(1)
        const input = mutation.mutateAsync.mock.calls[0][0]
        expect(input).toMatchObject({
            pdfFile: validFile,
            contractNo: 'CT-2026-01',
            customerId: 'c1',
            customerName: '客户甲',
            settlementPartyName: '主体乙',
            paymentTerms: '按合同约定',
            signedAt: '2026-01-01',
            validFrom: '2026-01-01',
            validTo: '2027-01-01',
        })
        expect(String(input.idempotencyKey)).toMatch(/^upload-/)
        expect(onOpenChange).toHaveBeenCalledWith(false)
        expect(onSuccess).toHaveBeenCalledWith(uploaded)
        expect(mutation.reset).toHaveBeenCalled()
    })

    it('skips submission when no PDF file is attached', async () => {
        const mutation: UploadMutationMock = makeUploadMutationMock()
        mockedUploadMutation.mockReturnValue(
            mutation as unknown as ReturnType<
                typeof useUploadContractPdfMutation
            >,
        )

        const onOpenChange = vi.fn()
        const { result, rerender } = renderHook(
            ({ open }: { open: boolean }) =>
                useContractUploadForm({
                    open,
                    onOpenChange,
                    initialCustomerId: '',
                }),
            { initialProps: { open: false } },
        )

        act(() => {
            rerender({ open: true })
        })

        await act(async () => {
            await result.current.form.handleSubmit()
        })

        expect(mutation.mutateAsync).not.toHaveBeenCalled()
        expect(onOpenChange).not.toHaveBeenCalled()
    })
})

describe('uploadSchema', () => {
    it('flags a missing PDF file', () => {
        const result = uploadSchema.safeParse({
            ...validValues,
            pdfFile: null,
        })
        expect(result.success).toBe(false)
        if (result.success) return
        const pdfIssue = result.error.issues.find(
            (issue) => issue.path[0] === 'pdfFile',
        )
        expect(pdfIssue?.message).toBe('请上传合同 PDF')
    })

    it('flags an empty contract number', () => {
        const result = uploadSchema.safeParse({
            ...validValues,
            contractNo: '   ',
        })
        expect(result.success).toBe(false)
        if (result.success) return
        expect(
            result.error.issues.some(
                (issue) =>
                    issue.path[0] === 'contractNo' &&
                    issue.message === '请填写合同编号',
            ),
        ).toBe(true)
    })

    it('flags validTo earlier than validFrom', () => {
        const result = uploadSchema.safeParse({
            ...validValues,
            validFrom: '2026-06-01',
            validTo: '2026-01-01',
        })
        expect(result.success).toBe(false)
        if (result.success) return
        expect(
            result.error.issues.some(
                (issue) =>
                    issue.path[0] === 'validTo' &&
                    issue.message === '有效期止不能早于有效期起',
            ),
        ).toBe(true)
    })

    it('accepts a fully valid input', () => {
        const result = uploadSchema.safeParse(validValues)
        expect(result.success).toBe(true)
    })
})

describe('uploadErrorMessage', () => {
    it('maps CONTRACT_NO_EXISTS to a business message', () => {
        expect(
            uploadErrorMessage({
                kind: 'Http',
                status: 409,
                message: 'CONTRACT_NO_EXISTS',
            }),
        ).toBe('该合同编号已存在，请打开已有合同核对；重复编号不能新建合同。')
    })

    it('maps CONTRACT_VALIDITY_INVALID to a business message', () => {
        expect(
            uploadErrorMessage({
                kind: 'Validation',
                status: 400,
                message: 'CONTRACT_VALIDITY_INVALID',
            }),
        ).toBe('有效期止不能早于有效期起。')
    })

    it('passes through ordinary messages', () => {
        expect(uploadErrorMessage(new Error('网络连接失败'))).toBe(
            '网络连接失败',
        )
    })

    it('falls back to the retry wording for unknown errors', () => {
        expect(uploadErrorMessage(undefined)).toBe(
            '上传失败，请使用原任务号重试。',
        )
    })
})
