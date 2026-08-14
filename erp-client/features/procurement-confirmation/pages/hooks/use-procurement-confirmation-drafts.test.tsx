import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"
import type { Dispatch, SetStateAction } from "react"

import type { ConfirmationLineDraft } from "@/features/procurement-confirmation/types"
import { useProcurementConfirmationDrafts } from "./use-procurement-confirmation-drafts"
import {
    makeRecommendation,
    makeSupplierOption,
    makeSupplyOption,
    makeTask,
} from "./test-data"

function renderDrafts(overrides: {
    task?: ReturnType<typeof makeTask> | undefined
    confirmOpen?: boolean
    recommendation?: ReturnType<typeof makeRecommendation> | undefined
    supplyOptions?: ReturnType<typeof makeSupplyOption>[]
    supplierOptions?: ReturnType<typeof makeSupplierOption>[]
} = {}) {
    const state = {
        saveMessage: null as string | null,
        actionError: null as string | null,
    }
    const setters = {
        setSaveMessage: vi.fn((next: string | null) => {
            state.saveMessage = next
        }) as unknown as Dispatch<SetStateAction<string | null>>,
        setActionError: vi.fn((next: string | null) => {
            state.actionError = next
        }) as unknown as Dispatch<SetStateAction<string | null>>,
    }
    const utils = renderHook((props: typeof overrides) => {
        return useProcurementConfirmationDrafts({
            task: props.task,
            confirmOpen: props.confirmOpen ?? false,
            recommendation: props.recommendation,
            supplyOptions: props.supplyOptions ?? [],
            supplierOptions: props.supplierOptions ?? [],
            setSaveMessage: setters.setSaveMessage,
            setActionError: setters.setActionError,
        })
    }, { initialProps: overrides })
    return { ...utils, state, setters }
}

const validDraft: ConfirmationLineDraft = {
    lineKey: "cl_1",
    submissionLineId: "sub_1",
    supplierId: "sup_1",
    supplierName: "演示供应商",
    offeringRevisionId: "off_1",
    confirmedQuantity: "10",
    latestCostGross: "80",
    inputTaxRate: "0.13",
    expectedDeliveryDate: "2026-08-20",
    fulfillmentMode: "WAREHOUSE",
    capabilityRevisionId: "cap_1",
    capabilitySummary: "当前有效供应商能力",
    qualificationStatus: "VALID",
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useProcurementConfirmationDrafts", () => {
    it("clones the current task confirmation lines as drafts", () => {
        const task = makeTask()
        const { result } = renderDrafts({ task })
        expect(result.current.lineDrafts).toHaveLength(1)
        expect(result.current.lineDrafts[0]).toEqual(
            task.confirmation.lines[0],
        )
        expect(result.current.lineDrafts[0]).not.toBe(
            task.confirmation.lines[0],
        )
        expect(result.current.dirty).toBe(false)
    })

    it("resets drafts when the task changes", () => {
        const taskA = makeTask()
        const taskB = makeTask({
            workItemId: "wi_2",
            confirmation: {
                confirmationId: "conf_2",
                status: "PENDING",
                editVersion: 1,
                lines: [
                    { ...validDraft, lineKey: "cl_2", confirmedQuantity: "3" },
                ],
            },
        })
        const { result, rerender } = renderDrafts({ task: taskA })
        act(() => {
            result.current.updateLine("cl_1", { confirmedQuantity: "7" })
        })
        expect(result.current.dirty).toBe(true)
        rerender({ task: taskB })
        expect(result.current.lineDrafts[0].lineKey).toBe("cl_2")
        expect(result.current.dirty).toBe(false)
    })

    it("clears drafts without a task", () => {
        const { result } = renderDrafts({})
        expect(result.current.lineDrafts).toEqual([])
        expect(result.current.dirty).toBe(false)
    })

    it("loads the recommendation into drafts once when the confirm dialog opens", () => {
        const task = makeTask()
        const recommendation = makeRecommendation()
        const { result, rerender } = renderDrafts({
            task,
            confirmOpen: true,
            recommendation,
        })
        expect(result.current.lineDrafts[0].lineKey).toBe("rec_1")
        expect(result.current.dirty).toBe(false)
        rerender({ task, confirmOpen: true, recommendation })
        expect(result.current.lineDrafts[0].lineKey).toBe("rec_1")
    })

    it("keeps user edits when the confirm dialog reopens", () => {
        const task = makeTask()
        const recommendation = makeRecommendation()
        const { result, rerender } = renderDrafts({
            task,
            confirmOpen: true,
            recommendation,
        })
        act(() => {
            result.current.updateLine("rec_1", { confirmedQuantity: "9" })
        })
        expect(result.current.dirty).toBe(true)
        rerender({
            task,
            confirmOpen: false,
            recommendation,
        })
        rerender({ task, confirmOpen: true, recommendation })
        expect(result.current.lineDrafts[0].confirmedQuantity).toBe("9")
    })

    it("backfills supplier names without clobbering edited drafts", () => {
        const task = makeTask({
            confirmation: {
                confirmationId: "conf_1",
                status: "PENDING",
                editVersion: 2,
                lines: [{ ...validDraft, supplierName: "供应商名称加载中" }],
            },
        })
        const { result, rerender } = renderDrafts({
            task,
            supplierOptions: [],
        })
        expect(result.current.lineDrafts[0].supplierName).toBe(
            "供应商名称加载中",
        )
        rerender({
            task,
            supplierOptions: [
                makeSupplierOption({ supplierName: "甲公司" }),
            ],
        })
        expect(result.current.lineDrafts[0].supplierName).toBe("甲公司")
    })

    it("computes coverage with confirmed/gap numbers per submission line", () => {
        const { result } = renderDrafts({
            task: makeTask({
                confirmation: {
                    confirmationId: "conf_1",
                    status: "PENDING",
                    editVersion: 2,
                    lines: [
                        { ...validDraft, confirmedQuantity: "4" },
                        {
                            ...validDraft,
                            lineKey: "cl_2",
                            confirmedQuantity: "3",
                        },
                    ],
                },
            }),
        })
        expect(result.current.coverage).toEqual([
            {
                submissionLineId: "sub_1",
                itemName: "演示商品",
                confirmed: "7",
                required: "10",
                complete: false,
                gap: "3",
            },
        ])
        expect(result.current.linesValid).toBe(true)
        expect(result.current.allCovered).toBe(false)
        expect(result.current.clientBlocking).toHaveLength(1)
        expect(result.current.clientBlocking[0].message).toBe(
            "已确认 7/10，缺口 3",
        )
    })

    it("treats a fully covered and valid draft as approvable", () => {
        const { result } = renderDrafts({ task: makeTask() })
        expect(result.current.coverage[0].complete).toBe(true)
        expect(result.current.linesValid).toBe(true)
        expect(result.current.allCovered).toBe(true)
        expect(result.current.clientBlocking).toEqual([])
    })

    it("flags incomplete drafts as invalid", () => {
        const { result } = renderDrafts({
            task: makeTask({
                confirmation: {
                    confirmationId: "conf_1",
                    status: "PENDING",
                    editVersion: 2,
                    lines: [{ ...validDraft, confirmedQuantity: "" }],
                },
            }),
        })
        expect(result.current.linesValid).toBe(false)
        expect(result.current.allCovered).toBe(false)
    })

    it("sums purchase gross including service fees and warehouse freight", () => {
        const { result } = renderDrafts({
            task: makeTask(),
            supplyOptions: [makeSupplyOption()],
        })
        // 10 × 80 + 服务费 3 + 入仓运费 12 = 815；毛利 = 1000 - 815
        expect(result.current.currentPlanSummary).toEqual({
            purchaseGross: 815,
            grossMargin: 185,
            orderCount: 1,
        })
    })

    it("updateLine patches the target line and marks dirty", () => {
        const { result } = renderDrafts({ task: makeTask() })
        act(() => {
            result.current.updateLine("cl_1", { expectedDeliveryDate: "2026-09-01" })
        })
        expect(result.current.lineDrafts[0].expectedDeliveryDate).toBe(
            "2026-09-01",
        )
        expect(result.current.dirty).toBe(true)
    })

    it("updatePlanLine reprices using the bulk tier when quantities reach the minimum", () => {
        const { result } = renderDrafts({
            task: makeTask({
                confirmation: {
                    confirmationId: "conf_1",
                    status: "PENDING",
                    editVersion: 2,
                    lines: [{ ...validDraft, confirmedQuantity: "2" }],
                },
            }),
            supplyOptions: [makeSupplyOption()],
        })
        // 2 件未达起订量 5 → 一件代发价 90
        act(() => {
            result.current.updatePlanLine("cl_1", { confirmedQuantity: "6" })
        })
        // 6 件达到起订量 → 集采价 80
        expect(result.current.lineDrafts[0].latestCostGross).toBe("80")
        expect(result.current.dirty).toBe(true)
    })

    it("applyRecommendation errors when the plan is not ready", () => {
        const { result, state } = renderDrafts({
            task: makeTask(),
            recommendation: makeRecommendation({ ready: false }),
        })
        act(() => {
            result.current.applyRecommendation()
        })
        expect(state.actionError).toBe(
            "当前没有可执行的系统采购方案，请先处理阻断项",
        )
        expect(result.current.dirty).toBe(false)
    })

    it("applyRecommendation loads the plan and marks dirty", () => {
        const { result, state } = renderDrafts({
            task: makeTask(),
            recommendation: makeRecommendation(),
        })
        act(() => {
            result.current.applyRecommendation()
        })
        expect(result.current.lineDrafts[0].lineKey).toBe("rec_1")
        expect(result.current.dirty).toBe(true)
        expect(state.saveMessage).toBe(
            "已重新载入系统最低成本方案，请核对交期后保存",
        )
    })

    it("addSplitLine appends an empty INVALID draft for a known submission line", () => {
        const { result } = renderDrafts({ task: makeTask() })
        act(() => {
            result.current.addSplitLine("sub_1")
        })
        expect(result.current.lineDrafts).toHaveLength(2)
        const added = result.current.lineDrafts[1]
        expect(added.submissionLineId).toBe("sub_1")
        expect(added.supplierId).toBe("")
        expect(added.fulfillmentMode).toBe("WAREHOUSE")
        expect(added.qualificationStatus).toBe("INVALID")
        expect(result.current.dirty).toBe(true)
    })

    it("addSplitLine ignores unknown submission lines", () => {
        const { result } = renderDrafts({ task: makeTask() })
        act(() => {
            result.current.addSplitLine("sub_missing")
        })
        expect(result.current.lineDrafts).toHaveLength(1)
    })

    it("removeLine keeps at least one line per submission line", () => {
        const task = makeTask({
            confirmation: {
                confirmationId: "conf_1",
                status: "PENDING",
                editVersion: 2,
                lines: [validDraft],
            },
        })
        const { result } = renderDrafts({ task })
        act(() => {
            result.current.addSplitLine("sub_1")
        })
        act(() => {
            result.current.removeLine(result.current.lineDrafts[0].lineKey)
        })
        expect(result.current.lineDrafts).toHaveLength(1)
    })

    it("builds offering options for a sku with supplier names and prices", () => {
        const { result } = renderDrafts({
            task: makeTask(),
            supplyOptions: [makeSupplyOption()],
            supplierOptions: [makeSupplierOption({ supplierName: "甲公司" })],
        })
        const options = result.current.offeringOptionsForSku("sku_1")
        expect(options).toHaveLength(1)
        expect(options[0].value).toBe("off_1")
        expect(options[0].label).toContain("甲公司")
        expect(options[0].label).toContain("90.00")
    })

    it("returns capability options only for the matching fulfillment mode", () => {
        const { result } = renderDrafts({
            task: makeTask(),
            supplyOptions: [makeSupplyOption()],
        })
        expect(
            result.current.capabilityOptionsForOffering("off_1", "WAREHOUSE"),
        ).toEqual([{ value: "cap_1", label: "实物商品" }])
        expect(
            result.current.capabilityOptionsForOffering("off_1", "ELECTRONIC"),
        ).toEqual([{ value: "cap_2", label: "虚拟商品" }])
    })

    it("lists all fulfillment modes for an unknown offering", () => {
        const { result } = renderDrafts({
            task: makeTask(),
            supplyOptions: [makeSupplyOption()],
        })
        const modes = result.current.fulfillmentOptionsForOffering("off_x")
        expect(modes.map((m) => m.value)).toEqual([
            "WAREHOUSE",
            "SUPPLIER_DIRECT",
            "ELECTRONIC",
            "SERVICE",
        ])
    })
})
