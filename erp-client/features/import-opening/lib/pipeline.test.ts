import { describe, expect, it } from "vitest"

import {
    PIPELINE_ORDER,
    buildStageStates,
    importStageLabels,
} from "@/features/import-opening/lib/pipeline"

describe("buildStageStates", () => {
    it("marks the receive stage as current at the start", () => {
        const states = buildStageStates("RECEIVE")
        expect(states.upload.status).toBe("current")
        expect(states.mapping.status).toBe("pending")
        expect(states.validation.status).toBe("pending")
        expect(states.preview.status).toBe("pending")
        expect(states.submission.status).toBe("pending")
        expect(states.result.status).toBe("pending")
    })

    it("marks earlier stages complete and later stages pending", () => {
        const states = buildStageStates("TRIAL")
        expect(states.upload.status).toBe("complete")
        expect(states.mapping.status).toBe("complete")
        expect(states.validation.status).toBe("current")
        expect(states.preview.status).toBe("pending")
        expect(states.submission.status).toBe("pending")
        expect(states.result.status).toBe("pending")
    })

    it("marks everything complete when the result stage is reached", () => {
        const states = buildStageStates("RESULT")
        expect(states.upload.status).toBe("complete")
        expect(states.mapping.status).toBe("complete")
        expect(states.validation.status).toBe("complete")
        expect(states.preview.status).toBe("complete")
        expect(states.submission.status).toBe("complete")
        expect(states.result.status).toBe("current")
    })

    it("attaches the pipeline stage labels as descriptions", () => {
        const states = buildStageStates("TRIAL")
        expect(states.upload.description).toBe("安全接收")
        expect(states.validation.description).toBe("业务校验与试算")
        expect(states.result.description).toBe("结果")
    })

    it("covers all six stages in the documented order", () => {
        expect(PIPELINE_ORDER).toEqual([
            "RECEIVE",
            "VALIDATE",
            "TRIAL",
            "CONFIRM",
            "APPLY",
            "RESULT",
        ])
        expect(Object.keys(importStageLabels)).toEqual([
            "upload",
            "mapping",
            "validation",
            "preview",
            "submission",
            "result",
        ])
    })
})
