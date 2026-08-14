import { describe, it, expect, vi, afterEach } from 'vitest'
import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { ReactElement } from 'react'
import type { CellContext, ColumnDef } from '@tanstack/react-table'
import type { ReadonlyURLSearchParams } from 'next/navigation'

import { useMallSyncColumns } from '@/features/mall-sync/hooks/mall-sync-columns'
import type {
    MallSnapshotRow,
    MallSyncJobRow,
    MappingTaskView,
    ReconciliationDifference,
} from '@/features/mall-sync/types'

afterEach(cleanup)

vi.mock('next/link', async () => {
    const React = await import('react')
    return {
        default: ({
            href,
            children,
        }: {
            href: string
            children?: React.ReactNode
        }) => React.createElement('a', { href }, children),
    }
})

vi.mock('next/navigation', () => ({
    useRouter: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }),
    useSearchParams: () => new URLSearchParams(),
    usePathname: () => '/test',
    useParams: () => ({}),
}))

const searchParams = new URLSearchParams() as unknown as ReadonlyURLSearchParams

function cellContext<TData>(
    row: TData,
): CellContext<TData, unknown> {
    return { row: { original: row } } as CellContext<TData, unknown>
}

function renderCell<TData>(
    column: ColumnDef<TData, unknown>,
    row: TData,
) {
    if (typeof column.cell !== 'function') {
        throw new Error(`cell renderer missing for ${column.id}`)
    }
    const element = column.cell(cellContext(row)) as ReactElement
    return render(element)
}

const baseJob: MallSyncJobRow = {
    jobId: 'job-1',
    jobNo: 'JOB-1',
    jobType: 'INCREMENTAL',
    jobTypeLabel: '增量拉取',
    status: 'SUCCEEDED',
    statusLabel: '成功',
    statusTone: 'success',
    pageCount: 3,
    itemCount: 41,
    errorCount: 0,
    startedAt: '2026-08-14T08:00:00.000Z',
    finishedAt: '2026-08-14T08:05:00.000Z',
    triggeredBy: '系统调度',
    watermarkAdvanced: true,
    allowedActions: [],
    actionBlockers: [],
}

const baseSnapshot: MallSnapshotRow = {
    snapshotId: 'snap-1',
    externalOrderNo: 'SO-2026-001',
    sourceUpdatedAt: '2026-08-14T08:00:00.000Z',
    observedAt: '2026-08-14T08:01:00.000Z',
    sourceStatusCode: 'paid',
    sourceStatusLabel: 'paid',
    contentHashShort: 'abc123…',
    mappingStatus: 'APPLIED',
    mappingStatusLabel: '已应用',
    appliedSalesOrderId: 'so-1',
    appliedSalesOrderNo: 'SO-ERP-1',
    syncJobId: 'job-1',
    syncJobNo: 'JOB-1',
    conflictFlags: [],
    whitelistFields: [],
}

const baseMappingBase = {
    mappingTaskId: 'mt-1',
    sourceSnapshotId: 'snap-1',
    externalOrderNo: 'SO-2026-001',
    mappingType: 'CUSTOMER' as const,
    mappingTypeLabel: '客户映射',
    mappingTaskStatus: 'PENDING' as const,
    mappingTaskStatusLabel: '待处理',
    sourceEvidence: [],
    candidateTargets: [],
    currentTargets: [],
    impactSummary: '客户身份未归属',
    resolutionHistory: [],
    allowedActions: ['CONFIRM_TARGET'],
    actionBlockers: [],
    lockVersion: 1,
    hasConflict: false,
}

const configuredMapping: MappingTaskView = {
    ...baseMappingBase,
    ownerRoutingState: 'CONFIGURED',
    ownerRole: 'SALES',
    ownerRoleLabel: '销售',
    workItem: {
        workItemId: 'wi-1',
        workItemType: 'BUSINESS_EXCEPTION',
        businessObjectType: 'MASTER_MAPPING_TASK',
        businessObjectId: 'mt-1',
        subjectVersion: 'sv-1',
        taskVersion: 'tv-1',
        status: 'OPEN',
        statusLabel: '处理中',
        assignmentMode: 'POOL',
        processingState: 'READY',
        allowedActions: ['START_PROCESSING'],
    },
}

const missingMapping: MappingTaskView = {
    ...baseMappingBase,
    mappingTaskId: 'mt-2',
    ownerRoutingState: 'MISSING',
}

const baseDiff: ReconciliationDifference = {
    differenceId: 'd-1',
    externalOrderNo: 'SO-2026-002',
    differenceType: 'FINGERPRINT',
    differenceTypeLabel: '内容不一致',
    sourceFingerprintShort: 'aaa111',
    erpFingerprintShort: 'bbb222',
    status: 'OPEN',
    statusLabel: '待处理',
    statusTone: 'warning',
    impactSummary: '来源与 ERP 内容不一致',
}

function renderColumns() {
    const patchUrl = vi.fn()
    const { result } = renderHook(() =>
        useMallSyncColumns({ patchUrl, searchParams }),
    )
    return { patchUrl, result }
}

describe('useMallSyncColumns', () => {
    it('builds the job columns with expected ids and headers', () => {
        const { result } = renderColumns()

        const columns = result.current.jobColumns
        expect(columns.map((c) => c.id)).toEqual([
            'jobNo',
            'type',
            'status',
            'counts',
            'wm',
            'started',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '任务号',
            '类型',
            '状态',
            '页 / 条 / 错',
            '同步进度',
            '开始',
        ])
    })

    it('builds snapshot, mapping and diff column ids', () => {
        const { result } = renderColumns()

        expect(result.current.snapshotColumns.map((c) => c.id)).toEqual([
            'order',
            'status',
            'mapping',
            'hash',
            'applied',
        ])
        expect(result.current.mappingColumns.map((c) => c.id)).toEqual([
            'order',
            'type',
            'mapStatus',
            'reapply',
            'owner',
            'wi',
        ])
        expect(result.current.diffColumns.map((c) => c.id)).toEqual([
            'order',
            'type',
            'fp',
            'status',
        ])
    })

    it('opens the jobs view for the clicked job row', () => {
        const { patchUrl, result } = renderColumns()
        const column = result.current.jobColumns.find((c) => c.id === 'jobNo')
        const view = renderCell(column!, baseJob)
        const button = view.getByRole('button')

        fireEvent.click(button)
        expect(patchUrl).toHaveBeenCalledWith({
            view: 'jobs',
            jobId: 'job-1',
        })
    })

    it('opens the snapshots view for the clicked snapshot row', () => {
        const { patchUrl, result } = renderColumns()
        const column = result.current.snapshotColumns.find(
            (c) => c.id === 'order',
        )
        const view = renderCell(column!, baseSnapshot)
        const button = view.getByRole('button')

        fireEvent.click(button)
        expect(patchUrl).toHaveBeenCalledWith({
            view: 'snapshots',
            snapshotId: 'snap-1',
        })
    })

    it('passes the workItemId when the mapping task is configured', () => {
        const { patchUrl, result } = renderColumns()
        const column = result.current.mappingColumns.find(
            (c) => c.id === 'order',
        )
        const view = renderCell(column!, configuredMapping)

        fireEvent.click(view.getByRole('button'))
        expect(patchUrl).toHaveBeenCalledWith({
            view: 'mapping',
            mappingTaskId: 'mt-1',
            workItemId: 'wi-1',
        })
    })

    it('clears the workItemId when owner routing is missing', () => {
        const { patchUrl, result } = renderColumns()
        const column = result.current.mappingColumns.find(
            (c) => c.id === 'order',
        )
        const view = renderCell(column!, missingMapping)

        fireEvent.click(view.getByRole('button'))
        expect(patchUrl).toHaveBeenCalledWith({
            view: 'mapping',
            mappingTaskId: 'mt-2',
            workItemId: null,
        })
    })

    it('opens the reconciliation view for the clicked difference row', () => {
        const { patchUrl, result } = renderColumns()
        const column = result.current.diffColumns.find(
            (c) => c.id === 'order',
        )
        const view = renderCell(column!, baseDiff)

        fireEvent.click(view.getByRole('button'))
        expect(patchUrl).toHaveBeenCalledWith({
            view: 'reconciliation',
            differenceId: 'd-1',
        })
    })

    it('shows the data-version bridge for difference rows with both fingerprints', () => {
        const { result } = renderColumns()
        const column = result.current.diffColumns.find((c) => c.id === 'fp')

        const view = renderCell(column!, baseDiff)
        expect(view.container.textContent).toContain('aaa111')
        expect(view.container.textContent).toContain('bbb222')
    })

    it('renders the applied ERP version link when present and a placeholder otherwise', () => {
        const { result } = renderColumns()
        const column = result.current.snapshotColumns.find(
            (c) => c.id === 'applied',
        )

        const withLink = renderCell(column!, baseSnapshot)
        expect(withLink.getByRole('link').getAttribute('href')).toBe(
            '/sales/orders/so-1',
        )

        const without = renderCell(column!, {
            ...baseSnapshot,
            appliedSalesOrderId: undefined,
            appliedSalesOrderNo: undefined,
        })
        expect(without.container.textContent).toContain('未形成')
    })
})
