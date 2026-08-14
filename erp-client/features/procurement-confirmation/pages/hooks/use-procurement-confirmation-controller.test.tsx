import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, cleanup } from "@testing-library/react"

import { renderHookWithProviders } from "@/features/test-utils"
import { useProcurementConfirmationController } from "./use-procurement-confirmation-controller"
import {
    makeQueueView,
    makeRecommendation,
    makeSupplierOption,
    makeSupplyOption,
    makeTask,
} from "./test-data"

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => "/procurement/confirm",
    useParams: () => ({}),
}))

const queryMocks = vi.hoisted(() => ({
    queue: vi.fn(),
    recommendation: vi.fn(),
    supplyOptions: vi.fn(),
    save: vi.fn(),
    complete: vi.fn(),
}))

vi.mock("@/features/procurement-confirmation/hooks/queries", () => ({
    useProcurementConfirmationQuery: queryMocks.queue,
    useProcurementRecommendationQuery: queryMocks.recommendation,
    useProcurementSupplyOptionsQuery: queryMocks.supplyOptions,
    useSaveProcurementConfirmationMutation: queryMocks.save,
    useCompleteProcurementMutation: queryMocks.complete,
}))

const supplierOptionMocks = vi.hoisted(() => ({
    suppliers: vi.fn(),
}))

vi.mock("@/hooks/use-options", () => ({
    useSupplierOptionsQuery: supplierOptionMocks.suppliers,
}))

const contractMocks = vi.hoisted(() => ({
    contract: vi.fn(),
}))

vi.mock("@/features/contracts/queries", () => ({
    useContractCenterQuery: contractMocks.contract,
}))

const responsibilityMocks = vi.hoisted(() => ({
    responsibility: vi.fn(),
}))

vi.mock("@/features/work-items", () => ({
    useWorkItemResponsibilityMutation: responsibilityMocks.responsibility,
}))

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.replace.mockClear()
    navMocks.push.mockClear()
    queryMocks.queue.mockReturnValue({
        isPending: false,
        isError: false,
        data: makeQueueView([makeTask()]),
        refetch: vi.fn(),
    })
    queryMocks.recommendation.mockReturnValue({
        isPending: false,
        isError: false,
        error: null,
        data: makeRecommendation(),
        refetch: vi.fn(),
    })
    queryMocks.supplyOptions.mockReturnValue({
        isPending: false,
        isError: false,
        data: [makeSupplyOption()],
    })
    queryMocks.save.mockReturnValue({ isPending: false, mutateAsync: vi.fn() })
    queryMocks.complete.mockReturnValue({
        isPending: false,
        mutateAsync: vi.fn(),
    })
    supplierOptionMocks.suppliers.mockReturnValue({
        data: [makeSupplierOption()],
    })
    contractMocks.contract.mockReturnValue({
        isPending: false,
        isError: false,
        data: undefined,
    })
    responsibilityMocks.responsibility.mockReturnValue({
        isPending: false,
        mutateAsync: vi.fn(),
    })
})

afterEach(() => {
    cleanup()
})

function renderController() {
    return renderHookWithProviders(() => useProcurementConfirmationController())
}

describe("useProcurementConfirmationController", () => {
    it("picks the task from the currentWorkItemId URL param", () => {
        const first = makeTask({ workItemId: "wi_1" })
        const second = makeTask({ workItemId: "wi_2" })
        navMocks.searchParams = new URLSearchParams(
            "scope=mine&currentWorkItemId=wi_2&queueContextId=queue:procurement-confirmation:mine",
        )
        queryMocks.queue.mockReturnValue({
            isPending: false,
            isError: false,
            data: makeQueueView([first, second], { currentWorkItemId: "wi_2" }),
            refetch: vi.fn(),
        })
        const { result } = renderController()
        expect(result.current.task?.workItemId).toBe("wi_2")
        expect(result.current.currentIndex).toBe(1)
    })

    it("falls back to view.current without a URL param", () => {
        const { result } = renderController()
        expect(result.current.task?.workItemId).toBe("wi_1")
    })

    it("reports completion when the queue view has no tasks", () => {
        queryMocks.queue.mockReturnValue({
            isPending: false,
            isError: false,
            data: makeQueueView([], { emptyReason: "NO_TASKS" }),
            refetch: vi.fn(),
        })
        const { result } = renderController()
        expect(result.current.completed).toBe(true)
        expect(result.current.task).toBeUndefined()
    })

    it("exposes the recommendation purchase estimate", () => {
        const { result } = renderController()
        expect(result.current.estimatedPurchase).toBe("800")
    })

    it("enables the recommendation query only while the confirm dialog is open", () => {
        const { result } = renderController()
        expect(queryMocks.recommendation).toHaveBeenLastCalledWith(
            "conf_1",
            false,
        )
        act(() => {
            result.current.setConfirmOpen(true)
        })
        expect(queryMocks.recommendation).toHaveBeenLastCalledWith(
            "conf_1",
            true,
        )
    })

    it("writes the queue defaults into the URL when params are missing", () => {
        renderController()
        const [url] = navMocks.replace.mock.lastCall ?? [""]
        expect(String(url ?? "")).toBe(
            "/procurement/confirm?scope=mine&queueContextId=queue%3Aprocurement-confirmation%3Amine&currentWorkItemId=wi_1",
        )
    })

    it("goToWorkItem switches the currentWorkItemId in the URL", () => {
        const { result } = renderController()
        act(() => {
            result.current.goToWorkItem("wi_9")
        })
        const [url] = navMocks.replace.mock.lastCall ?? [""]
        expect(String(url ?? "")).toContain("currentWorkItemId=wi_9")
    })

    it("returns no neighbour outside the task list", () => {
        const { result } = renderController()
        expect(result.current.neighborId(1)).toBeUndefined()
        expect(result.current.neighborId(-1)).toBeUndefined()
        expect(result.current.neighborId(0)).toBe("wi_1")
    })

    it("marks pending while a decision mutation is running", () => {
        queryMocks.complete.mockReturnValue({
            isPending: true,
            mutateAsync: vi.fn(),
        })
        const { result } = renderController()
        expect(result.current.formalPending).toBe(true)
    })

    it("blocks j/k navigation while the draft is dirty", () => {
        const { result } = renderController()
        act(() => {
            result.current.drafts.updateLine("cl_1", {
                confirmedQuantity: "9",
            })
        })
        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "j", bubbles: true }),
            )
        })
        expect(result.current.actionError).toBe(
            "有未保存修改，请先保存后再切换",
        )
    })

    it("clears the finished result strip on demand", () => {
        const { result } = renderController()
        expect(result.current.finishedResult).toBeNull()
        act(() => {
            result.current.setFinishedResult({
                status: "succeeded",
                title: "上一项已通过",
                description: "",
                stayOnItem: true,
            })
        })
        expect(result.current.finishedResult).not.toBeNull()
        act(() => {
            result.current.setFinishedResult(null)
        })
        expect(result.current.finishedResult).toBeNull()
    })
})
