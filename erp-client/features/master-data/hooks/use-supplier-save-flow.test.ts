import { describe, it, expect, vi, beforeEach } from "vitest"
import * as React from "react"
import { act, renderHook, waitFor } from "@testing-library/react"

import { useAppForm } from "@/components/form"
import { useSupplierSaveFlow } from "./use-supplier-save-flow"
import type { SupplierEditor } from "./use-supplier-editor"
import { createSupplierEditorDefaults } from "@/features/master-data/lib/supplier-editor-model"

function makeEditor(overrides: Partial<SupplierEditor> = {}): SupplierEditor {
    return {
        isCreate: true,
        router: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
        detailQuery: {
            refetch: vi.fn().mockResolvedValue({ data: undefined }),
        },
        data: undefined,
        form: {} as never,
        formError: null,
        setFormError: vi.fn(),
        result: null,
        setResult: vi.fn(),
        disableOpen: false,
        setDisableOpen: vi.fn(),
        discardOpen: false,
        setDiscardOpen: vi.fn(),
        saveReasonOpen: false,
        setSaveReasonOpen: vi.fn(),
        reasonDraft: "",
        setReasonDraft: vi.fn(),
        reasonError: null,
        setReasonError: vi.fn(),
        pendingNav: null,
        setPendingNav: vi.fn(),
        activeSection: "basic",
        setActiveSection: vi.fn(),
        errorRef: { current: null },
        editedSensitiveRef: { current: new Set() },
        pendingChangeReasonRef: { current: null },
        rememberMediaFiles: vi.fn(),
        mediaUrlsFor: vi.fn(() => ({})),
        mediaAssetIdsFor: vi.fn(() => ({})),
        initialFormValues: createSupplierEditorDefaults(true),
        navigateAway: vi.fn(),
        sensitiveByLabel: new Map(),
        listHref: "/master-data/suppliers",
        pending: false,
        canCreate: true,
        canRevise: false,
        canDisable: false,
        canRevealSensitive: false,
        reviseBlocker: undefined,
        disableBlocker: undefined,
        ...overrides,
    } as SupplierEditor
}

/** 挂载真实表单与保存流；reasonDraft 走 React state，保证回调读到最新值。 */
function setup(editorOverrides: Partial<SupplierEditor> = {}) {
    const setFormError = vi.fn()
    const setSaveReasonOpen = vi.fn()
    const setReasonError = vi.fn()
    const submit = vi.fn()

    const { result } = renderHook(() => {
        const form = useAppForm({
            defaultValues: createSupplierEditorDefaults(true),
            onSubmit: async ({ value }) => {
                submit(value)
            },
        })
        const [reasonDraft, setReasonDraft] = React.useState("")
        const [reasonError, setReasonErrorState] = React.useState<
            string | null
        >(null)
        const editor = makeEditor({
            form,
            setFormError,
            setSaveReasonOpen,
            setReasonDraft,
            setReasonError: (error: React.SetStateAction<string | null>) => {
                setReasonError(error)
                setReasonErrorState(error)
            },
            reasonDraft,
            reasonError,
            ...editorOverrides,
        })
        const flow = useSupplierSaveFlow(editor)
        return { flow, form, setReasonDraft }
    })

    return { result, setFormError, setSaveReasonOpen, setReasonError, submit }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useSupplierSaveFlow", () => {
    it("derives summary rows from the current form values", () => {
        const { result } = setup()
        act(() => {
            result.current.form.setFieldValue("contactName", "张三")
            result.current.form.setFieldValue("settlement", "月结 30 天")
        })
        const rows = result.current.flow.summaryRows
        expect(rows.find((row) => row.label === "联系人")?.value).toBe("张三")
        expect(rows.find((row) => row.label === "结算方式")?.value).toBe(
            "月结 30 天",
        )
        expect(
            rows.filter((row) => row.value === "—"),
        ).toHaveLength(2)
    })

    it("requestSave reports validation errors without opening the reason dialog", () => {
        const { result, setFormError, setSaveReasonOpen } = setup()
        const event = { preventDefault: vi.fn() }
        act(() => {
            result.current.flow.requestSave(event as never)
        })
        expect(event.preventDefault).toHaveBeenCalled()
        expect(setFormError).toHaveBeenCalledWith("请填写供应商名称")
        expect(setSaveReasonOpen).not.toHaveBeenCalled()
    })

    it("requestSave opens the reason dialog with the default create reason", () => {
        const { result, setFormError, setSaveReasonOpen } = setup()
        act(() => {
            result.current.form.setFieldValue("name", "示例供应商")
            result.current.form.setFieldValue("company", "示例企业有限公司")
            result.current.form.setFieldValue("signingEntity", "福尚云")
            result.current.form.setFieldValue("paymentEntity", "福尚云")
        })
        const event = { preventDefault: vi.fn() }
        act(() => {
            result.current.flow.requestSave(event as never)
        })
        expect(setFormError).toHaveBeenCalledWith(null)
        expect(setSaveReasonOpen).toHaveBeenCalledWith(true)
        expect(result.current.flow.summaryRows[0].value).toBe("—")
    })

    it("confirmSaveWithReason rejects a too-short reason", () => {
        const { result, setReasonError, submit } = setup()
        act(() => {
            result.current.setReasonDraft("改")
        })
        act(() => {
            result.current.flow.confirmSaveWithReason()
        })
        expect(setReasonError).toHaveBeenCalledWith(
            "请填写本次保存的变更原因",
        )
        expect(submit).not.toHaveBeenCalled()
    })

    it("confirmSaveWithReason submits the form with the confirmed reason", async () => {
        const { result, submit } = setup()
        act(() => {
            result.current.setReasonDraft("新增供应商资质")
        })
        act(() => {
            result.current.flow.confirmSaveWithReason()
        })
        await waitFor(() => expect(submit).toHaveBeenCalledTimes(1))
        expect(submit.mock.calls[0]![0]).toMatchObject({
            changeReason: "新增供应商资质",
        })
    })

    it("resolves phone sensitivity preferring the 联系电话 label", () => {
        const contactPhone = { maskedValue: "138****" }
        const { result: prefersPhone } = setup({
            sensitiveByLabel: new Map([
                ["联系电话", contactPhone],
                ["联系人", { maskedValue: "张*" }],
            ]),
        })
        expect(prefersPhone.current.flow.phoneSensitive).toBe(contactPhone)

        const contactOnly = { maskedValue: "张*" }
        const { result: fallsBack } = setup({
            sensitiveByLabel: new Map([["联系人", contactOnly]]),
        })
        expect(fallsBack.current.flow.phoneSensitive).toBe(contactOnly)
    })

    it("refreshSensitiveToken refetches and finds the token by label", async () => {
        const refetch = vi.fn().mockResolvedValue({
            data: {
                sensitiveFields: [
                    {
                        label: "联系电话",
                        maskedValue: "138****",
                        revealToken: "tok-1",
                    },
                ],
            },
        })
        const { result } = setup({
            detailQuery: { refetch } as never,
        })
        let token: string | undefined
        await act(async () => {
            token = await result.current.flow.refreshSensitiveToken([
                "联系电话",
            ])
        })
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(token).toBe("tok-1")
    })
})
