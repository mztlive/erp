import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueQuery,
    CardFundsReviewQueueView,
    CompleteCardFundsReviewCommand,
    FormalActionResponse,
} from "@/features/card-funds-review/types"

/** 可配置的复核任务夹具；默认处于可执行全部动作的开放状态。 */
export function makeTask(
    overrides: {
        workItem?: Partial<CardFundsReviewItemView['workItem']>
        salesOrder?: Partial<CardFundsReviewItemView['salesOrder']>
        account?: Partial<CardFundsReviewItemView['account']>
        reviewChain?: Partial<CardFundsReviewItemView['reviewChain']>
        currentSalesOrderRevisionId?: string
        fundsFactVersion?: string
        receiptFacts?: CardFundsReviewItemView['receiptFacts']
        invoiceFacts?: CardFundsReviewItemView['invoiceFacts']
        difference?: CardFundsReviewItemView['difference']
        reviewType?: CardFundsReviewItemView['reviewType']
        fingerprintStatus?: CardFundsReviewItemView['fingerprintStatus']
        currentEvidence?: CardFundsReviewItemView['currentEvidence']
    } = {},
): CardFundsReviewItemView {
    return {
        workItem: {
            workItemId: "wi_1",
            taskVersion: "tv_1",
            workItemType: "CARD_FUNDS_REVIEW",
            subjectVersion: "sv_1",
            workItemStatus: "OPEN",
            assignmentMode: "DIRECT",
            allowedActions: [
                "START_PROCESSING",
                "CONFIRM_ZERO",
                "APPROVE",
                "REJECT",
                "RELEASE_TO_TEAM",
                "REGISTER_RECEIPT",
                "REGISTER_INVOICE",
            ],
            actionBlockers: [],
            reason: "期初票款待复核",
            impact: "复核通过后指标生效",
            priority: 80,
            ...overrides.workItem,
        },
        salesOrder: {
            id: "so_1",
            orderNo: "SO-2026-001",
            revisionNo: 3,
            snapshotAt: "2026-07-01T00:00:00.000Z",
            ...overrides.salesOrder,
        },
        account: {
            id: "acct_1",
            accountSeq: 1,
            domainVersion: "dv_1",
            customerId: "cust_1",
            customerName: "演示客户",
            counterpartyPartyId: "cp_1",
            counterpartyPartyName: "演示往来主体",
            mallName: "自营商城",
            reviewStatus: "pending",
            grossTotal: "1130.00",
            settledTotal: "0.00",
            openTotal: "1130.00",
            invoicedTotal: "0.00",
            openInvoiceableTotal: "1130.00",
            syncedGrossAmount: "1130.00",
            fundsReliability: "UNRELIABLE_PENDING_REVIEW",
            reliabilityNote: "卡券票款待复核，指标暂不可靠",
            ...overrides.account,
        },
        reviewChain: {
            tailReviewId: "rv_0",
            chainVersion: "cv_1",
            nextReviewNo: 2,
            items: [],
            ...overrides.reviewChain,
        },
        currentSalesOrderRevisionId:
            overrides.currentSalesOrderRevisionId ?? 'rev_9',
        fundsFactVersion: overrides.fundsFactVersion ?? 'ffv_1',
        receiptFacts: overrides.receiptFacts ?? [],
        invoiceFacts: overrides.invoiceFacts ?? [],
        difference: overrides.difference,
        reviewType: overrides.reviewType ?? 'OPENING',
        fingerprintStatus: overrides.fingerprintStatus ?? {
            label: '数据版本',
            tone: 'neutral',
            detail: 'subject=sv_1',
        },
        currentEvidence: overrides.currentEvidence ?? {
            evidenceDocumentIds: [],
            evidenceReferences: [],
            comment: undefined,
        },
    }
}

export function makeQueueView(
    task: CardFundsReviewItemView,
    overrides: Partial<CardFundsReviewQueueView> = {},
): CardFundsReviewQueueView {
    return {
        preferences: { autoNextDefault: true },
        context: {
            queueContextId: "queue:card-funds-review:mine",
            position: 1,
            total: 1,
            currentWorkItemId: task.workItem.workItemId,
            filterSummary: "仅我的 · 期初 · 待处理有效队列 · 全部时限",
            queueContextUpdatedAt: "2026-07-01T00:00:00.000Z",
            ...overrides.context,
        },
        tasks: [task],
        current: task,
        ...overrides,
    }
}

export function makeQueueQuery(
    overrides: Partial<CardFundsReviewQueueQuery> = {},
): CardFundsReviewQueueQuery {
    return {
        scope: "mine",
        type: "all",
        status: "OPEN",
        due: "all",
        ...overrides,
    }
}

export function makeCompleteCommand(
    overrides: Partial<CompleteCardFundsReviewCommand> = {},
): CompleteCardFundsReviewCommand {
    return {
        workItemId: "wi_1",
        expectedTaskVersion: "tv_1",
        expectedSubjectVersion: "sv_1",
        decision: {
            reviewResult: "APPROVED",
            conclusion: "RECORDED_FACTS_RECONCILED",
            receivableAccountId: "acct_1",
            expectedAccountSeq: 1,
            expectedAccountDomainVersion: "dv_1",
            expectedReviewChainVersion: "cv_1",
            expectedNextReviewNo: 2,
            expectedSalesOrderRevisionId: "rev_9",
            expectedFundsFactVersion: "ffv_1",
            reviewType: "OPENING",
            evidenceDocumentIds: [],
            evidenceReferences: ["测试证据"],
            comment: undefined,
        },
        idempotencyKey: "w13:wi_1:tv_1:approve:RECORDED_FACTS_RECONCILED",
        ...overrides,
    }
}

export function makeApprovedResponse(): FormalActionResponse {
    return {
        status: "succeeded",
        outcome: {
            kind: "APPROVED",
            business: {
                receivableFundsReviewId: "rfr_1",
                receivableAccountId: "acct_1",
                reviewNo: 7,
                accountReviewStatus: "reviewed",
                workflowActionId: "wa_1",
                operationId: "op_1",
                completedAt: "2026-07-01T08:00:00.000Z",
                reviewResult: "APPROVED",
                conclusion: "RECORDED_FACTS_RECONCILED",
            },
        },
    }
}
