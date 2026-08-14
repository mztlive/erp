import { describe, expect, it } from 'vitest'

import {
    buildStageStates,
    mapJobProgressStatus,
    newRequestId,
} from '@/features/history-backfill/lib/presentation'

describe("buildStageStates", () => {
    it("marks all stages complete and the final stage current for DONE", () => {
        const states = buildStageStates("DONE")

        expect(states.upload.status).toBe("complete")
        expect(states.mapping.status).toBe("complete")
        expect(states.validation.status).toBe("complete")
        expect(states.preview.status).toBe("complete")
        expect(states.submission.status).toBe("complete")
        expect(states.result.status).toBe("current")
    })

    it("marks only the first stage current for SCOPE", () => {
        const states = buildStageStates("SCOPE")

        expect(states.upload.status).toBe("current")
        expect(states.mapping.status).toBe("pending")
        expect(states.result.status).toBe("pending")
    })

    it("marks upstream complete and the active stage current for INGEST", () => {
        const states = buildStageStates("INGEST")

        expect(states.upload.status).toBe("complete")
        expect(states.mapping.status).toBe("complete")
        expect(states.validation.status).toBe("current")
        expect(states.preview.status).toBe("pending")
        expect(states.submission.status).toBe("pending")
        expect(states.result.status).toBe("pending")
    })

    it("carries the stage label as description", () => {
        const states = buildStageStates("REPORT")

        expect(states.submission.status).toBe("current")
        expect(states.submission.description).toBe("报告")
    })
})

describe("mapJobProgressStatus", () => {
    it("maps running-ish statuses to running", () => {
        expect(mapJobProgressStatus("RUNNING")).toBe("running")
        expect(mapJobProgressStatus("VALIDATING")).toBe("running")
    })

    it("maps terminal statuses", () => {
        expect(mapJobProgressStatus("COMPLETED")).toBe("succeeded")
        expect(mapJobProgressStatus("PARTIAL")).toBe("partial")
        expect(mapJobProgressStatus("FAILED")).toBe("failed")
    })

    it("maps pre-run statuses to queued", () => {
        expect(mapJobProgressStatus("DRAFT")).toBe("queued")
        expect(mapJobProgressStatus("READY")).toBe("queued")
    })
})

describe("newRequestId", () => {
    it("uses the given prefix and is unique per call", () => {
        const first = newRequestId("op")
        const second = newRequestId("op")

        expect(first).toMatch(/^op_/)
        expect(second).toMatch(/^op_/)
        expect(first).not.toBe(second)
    })
})
