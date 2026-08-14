import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import type {
    ConfirmationLineDraft,
    ProcurementConfirmationTask,
    ProcurementQueueView,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"

/** 可配置的采购确认任务夹具；默认有一条可直接确认的销售行。 */
export function makeTask(
    overrides: Partial<ProcurementConfirmationTask> = {},
): ProcurementConfirmationTask {
    const submissionLine = {
        submissionLineId: "sub_1",
        itemName: "演示商品",
        itemSku: "sku_1",
        committedQuantity: "10",
        unit: "件",
        requestedDeliveryDate: "2026-08-20",
        salesAmountGross: "1000",
    }
    const confirmationLine: ConfirmationLineDraft = {
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
    return {
        workItemId: "wi_1",
        taskVersion: "5",
        responsibilityScope: "mine",
        status: "OPEN",
        assignmentMode: "DIRECT",
        priority: 50,
        dueAt: "2026-08-14T10:00:00.000Z",
        impactSummary: "待采购二次确认",
        subjectVersion: "1",
        subjectHash: "sub_1",
        salesSubmission: {
            salesOrderId: "so_1",
            salesOrderNo: "SO-2026-001",
            submissionId: "sub_1",
            submissionNo: 1,
            subjectHash: "sub_1",
            subjectHashSummary: "sub_1",
            submittedAt: "2026-08-13T10:00:00.000Z",
            submittedByLabel: "销售提交人",
            customerSnapshot: "演示客户",
            paymentTermLabel: "月结30天",
            grossAmount: "1000",
            origin: "INITIAL",
            lines: [submissionLine],
        },
        confirmation: {
            confirmationId: "conf_1",
            status: "PENDING",
            editVersion: 2,
            lines: [confirmationLine],
        },
        decisionSummary: {
            coverageByLine: [],
            estimatedPurchaseGross: "800",
            blockingIssues: [],
            warnings: [],
        },
        allowedActions: [
            "START_PROCESSING",
            "SAVE",
            "APPROVE",
            "REJECT",
            "RELEASE_TO_TEAM",
        ],
        actionBlockers: [],
        riskLabel: "处理中",
        riskTone: "info",
        riskDescription: "待采购二次确认",
        ...overrides,
    }
}

export function makeQueueView(
    tasks: ProcurementConfirmationTask[],
    overrides: {
        currentWorkItemId?: string
        total?: number
        emptyReason?: ProcurementQueueView["emptyReason"]
    } = {},
): ProcurementQueueView {
    return {
        preferences: { autoNextDefault: true },
        context: {
            queueContextId: "queue:procurement-confirmation:mine",
            position: 1,
            total: overrides.total ?? tasks.length,
            currentWorkItemId: overrides.currentWorkItemId,
            filterSummary: "仅我的 · 有效全部 · 截止优先",
            queueContextUpdatedAt: "2026-08-14T10:00:00.000Z",
        },
        tasks,
        current:
            tasks.find(
                (task) => task.workItemId === overrides.currentWorkItemId,
            ) ?? tasks[0],
        emptyReason: overrides.emptyReason,
    }
}

/** 可配置的最低成本方案夹具；默认 ready 且覆盖 sub_1。 */
export function makeRecommendation(
    overrides: Partial<ProcurementRecommendation> = {},
): ProcurementRecommendation {
    return {
        confirmationId: "conf_1",
        policyVersion: "v1",
        calculatedAt: "2026-08-14T09:00:00.000Z",
        ready: true,
        lines: [
            {
                lineKey: "rec_1",
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
                itemName: "演示商品",
                itemSku: "sku_1",
                landedGross: "800",
                recommendationReason: "最低成本",
            },
        ],
        purchaseOrders: [
            {
                supplierId: "sup_1",
                supplierName: "演示供应商",
                fulfillmentMode: "WAREHOUSE",
                lineCount: 1,
                estimatedGross: "800",
            },
        ],
        estimatedPurchaseGross: "800",
        salesGross: "1000",
        estimatedGrossMargin: "200",
        blockingIssues: [],
        warnings: [],
        ...overrides,
    }
}

export function makeSupplyOption(
    overrides: Partial<ProcurementSupplyOption> = {},
): ProcurementSupplyOption {
    return {
        skuId: "sku_1",
        supplierId: "sup_1",
        offeringRevisionId: "off_1",
        offeringRevisionNo: 3,
        costGross: "80",
        bulkCostGross: "80",
        dropshipCostGross: "90",
        bulkMinimumOrderQuantity: "5",
        inputTaxRate: "0.13",
        freightAmount: "12",
        serviceFeeAmount: "3",
        capabilities: [
            { revisionId: "cap_1", label: "实物商品", capabilityCode: "physical" },
            { revisionId: "cap_2", label: "虚拟商品", capabilityCode: "virtual" },
        ],
        ...overrides,
    }
}

export function makeSupplierOption(overrides: {
    supplierId?: string
    supplierName?: string
} = {}) {
    return {
        supplierId: overrides.supplierId ?? "sup_1",
        supplierName: overrides.supplierName ?? "演示供应商",
    }
}
