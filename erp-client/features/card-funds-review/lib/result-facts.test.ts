import { describe, it, expect } from 'vitest'

import { buildResultFacts } from './result-facts'
import type { FormalOutcome } from '@/features/card-funds-review/types'

describe('buildResultFacts', () => {
    it('returns an empty list without an outcome', () => {
        expect(buildResultFacts(undefined)).toEqual([])
    })

    it('lists task and responsibility facts for a release', () => {
        const outcome: FormalOutcome = {
            kind: 'RELEASED_TO_TEAM',
            workItemId: 'wi_1',
            workItemStatus: 'OPEN',
            taskVersion: 'tv_3',
            reference: 'wi_1',
        }
        expect(buildResultFacts(outcome)).toEqual([
            { label: '任务状态', value: '开放' },
            { label: '责任状态', value: '已退回团队' },
            { label: '任务版本', value: 'tv_3' },
        ])
    })

    it('lists approval facts with the Chinese conclusion label', () => {
        const outcome: FormalOutcome = {
            kind: 'APPROVED',
            business: {
                receivableFundsReviewId: 'rfr_1',
                receivableAccountId: 'acct_1',
                reviewNo: 7,
                accountReviewStatus: 'reviewed',
                workflowActionId: 'wa_1',
                operationId: 'op_1',
                completedAt: '2026-07-01T08:00:00.000Z',
                reviewResult: 'APPROVED',
                conclusion: 'RECORDED_FACTS_RECONCILED',
            },
        }
        const facts = buildResultFacts(outcome)
        expect(facts).toHaveLength(4)
        expect(facts[0]).toEqual({ label: '复核号', value: '7' })
        expect(facts[1]).toEqual({
            label: '结论',
            value: '已登记记录并核对一致',
        })
        expect(facts[3]).toEqual({ label: '操作号', value: 'op_1' })
        expect(facts[2]?.label).toBe('完成时间')
    })

    it('labels rejected conclusions as 驳回', () => {
        const outcome: FormalOutcome = {
            kind: 'REJECTED',
            business: {
                receivableFundsReviewId: 'rfr_2',
                receivableAccountId: 'acct_1',
                reviewNo: 8,
                accountReviewStatus: 'rejected',
                workflowActionId: 'wa_2',
                operationId: 'op_2',
                completedAt: '2026-07-01T09:00:00.000Z',
                reviewResult: 'REJECTED',
                conclusion: 'REJECTED',
                followUpConfiguration: {
                    status: 'BLOCKED',
                    blockerCode: 'REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED',
                    collaborationMessage: '驳回后继未配置',
                    requiredRegistration: [],
                },
            },
        }
        const facts = buildResultFacts(outcome)
        expect(facts[1]).toEqual({ label: '结论', value: '驳回' })
    })
})
