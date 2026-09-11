import { describe, expect, it } from "vitest"

import {
    isProductImportActive,
    productImportProgressStatus,
    visibleProductImportJobs,
} from "./product-import"

describe("product import status", () => {
    it("maps backend status to progress presentation", () => {
        expect(productImportProgressStatus("pending")).toBe("queued")
        expect(productImportProgressStatus("running")).toBe("running")
        expect(productImportProgressStatus("partially_succeeded")).toBe(
            "partial",
        )
        expect(productImportProgressStatus("succeeded")).toBe("succeeded")
    })

    it("treats pending and running as active", () => {
        expect(isProductImportActive("pending")).toBe(true)
        expect(isProductImportActive("succeeded")).toBe(false)
    })

    it("hides finished jobs unless they were submitted this session", () => {
        const jobs = [
            { id: "active", status: "running" },
            { id: "old", status: "partially_succeeded" },
            { id: "mine", status: "succeeded" },
        ]
        expect(visibleProductImportJobs(jobs, ["mine"]).map((job) => job.id)).toEqual(
            ["active", "mine"],
        )
    })
})
