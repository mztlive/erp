import { describe, expect, it } from "vitest"

import {
    buildImportOpeningSearchParams,
    parseImportOpeningSearchParams,
} from "@/features/import-opening/lib/url-state"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"

function state(overrides?: Partial<ImportOpeningUrlState>): ImportOpeningUrlState {
    return {
        environment: "VALIDATION",
        section: "overview",
        page: 1,
        ...overrides,
    }
}

describe("parseImportOpeningSearchParams", () => {
    it("falls back to defaults for an empty query string", () => {
        expect(parseImportOpeningSearchParams(new URLSearchParams())).toEqual({
            environment: "VALIDATION",
            section: "overview",
            page: 1,
        })
    })

    it("parses every supported param", () => {
        const parsed = parseImportOpeningSearchParams(
            new URLSearchParams(
                "environment=PRODUCTION&status=RECEIVING&objectType=SKU&q=abc" +
                    "&batchId=b1&workItemId=w1&confirmationScope=SALES" +
                    "&queueContextId=q1&section=trial&issueCode=MAPPING_CONFLICT" +
                    "&issueObject=SKU&rowStatus=FAILED&page=3",
            ),
        )
        expect(parsed).toEqual({
            environment: "PRODUCTION",
            status: "RECEIVING",
            objectType: "SKU",
            q: "abc",
            batchId: "b1",
            workItemId: "w1",
            confirmationScope: "SALES",
            queueContextId: "q1",
            section: "trial",
            issueCode: "MAPPING_CONFLICT",
            issueObjectType: "SKU",
            rowStatus: "FAILED",
            page: 3,
        })
    })

    it("drops unknown enum values", () => {
        const parsed = parseImportOpeningSearchParams(
            new URLSearchParams(
                "section=bogus&objectType=NOPE&rowStatus=UNKNOWN&issueCode=JUNK&status=any",
            ),
        )
        expect(parsed.section).toBe("overview")
        expect(parsed.objectType).toBeUndefined()
        expect(parsed.rowStatus).toBeUndefined()
        expect(parsed.issueCode).toBeUndefined()
        expect(parsed.status).toBe("any")
    })

    it("normalizes the environment value", () => {
        expect(
            parseImportOpeningSearchParams(new URLSearchParams("environment=production"))
                .environment,
        ).toBe("PRODUCTION")
        expect(
            parseImportOpeningSearchParams(new URLSearchParams("environment=garbage"))
                .environment,
        ).toBe("VALIDATION")
    })

    it("clamps invalid page values to 1", () => {
        for (const raw of ["0", "-3", "abc", ""]) {
            const parsed = parseImportOpeningSearchParams(
                new URLSearchParams(`page=${raw}`),
            )
            expect(parsed.page, `page=${raw}`).toBe(1)
        }
    })

    it("prefers the primary key over the id alias", () => {
        const parsed = parseImportOpeningSearchParams(
            new URLSearchParams("batchId=b1&id=b2"),
        )
        expect(parsed.batchId).toBe("b1")
    })
})

describe("buildImportOpeningSearchParams", () => {
    it("builds an empty string for the default state", () => {
        expect(buildImportOpeningSearchParams(state())).toBe("")
    })

    it("writes only non-default fields in declaration order", () => {
        expect(
            buildImportOpeningSearchParams(
                state({
                    environment: "PRODUCTION",
                    batchId: "b1",
                    section: "trial",
                    page: 2,
                }),
            ),
        ).toBe("?environment=PRODUCTION&batchId=b1&section=trial&page=2")
    })

    it("omits section when no batch is open", () => {
        expect(
            buildImportOpeningSearchParams(
                state({ section: "trial", page: 1 }),
            ),
        ).toBe("")
    })

    it("round-trips a parsed state without losing parameters", () => {
        const qs =
            "environment=PRODUCTION&status=RECEIVING&objectType=SKU&q=abc" +
            "&batchId=b1&confirmationScope=SALES&queueContextId=q1" +
            "&section=trial&issueCode=MAPPING_CONFLICT&issueObject=SKU" +
            "&rowStatus=FAILED&page=3"
        const parsed = parseImportOpeningSearchParams(new URLSearchParams(qs))
        expect(buildImportOpeningSearchParams(parsed)).toBe(`?${qs}`)
    })

    it("keeps a blank q out of the URL", () => {
        expect(buildImportOpeningSearchParams(state({ q: "  " }))).toBe(
            "?q=++",
        )
    })
})
